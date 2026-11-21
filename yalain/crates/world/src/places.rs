//! Places: moving a player through portal space to another place (a portal's destination, its
//! lifestone), and the places a character keeps.
//!
//! **Using a portal.** The client asks to use an object; a player too far from it is first
//! walked to it (a movement order toward the object, ended when the player is in reach). Then,
//! in one batch: the portal flashes, the player is told it is teleporting, its teleport and
//! arrival effects play, its animation and movement freeze, and it enters portal space. A second
//! and a half later it leaves portal space, its animation is freed, the use is answered, and it is
//! placed at the destination with a newer teleport stamp: the clients that knew it at the old
//! place and do not see the new one see it leave, those at the new place are told of it, and it is
//! told of everything around the new place again. When its client has drawn the new place, its
//! movement is freed and the arrival effect ended (as at entry).
//!
//! **Walking into a portal.** A portal whose kind is used by walking into it (the world's
//! portals; not summoned ones, doors or shards) takes a player whose body comes to touch it as a use
//! does, with no request and no movement order: the portal's body a sphere of 0.878 m 0.841 m above
//! its place, the player's two of 0.555 m 0.55 m and 1.50 m above its feet. It fires as the bodies
//! come to touch, not again while they stay touching; leaving and coming back fires it again. The
//! use is answered as a use, from where the player stood as it touched.
//!
//! **Lifestones.** Using a lifestone binds the player to it: the lifestone and the player play
//! their binding behaviors and the use is answered with the saved message. Recalling (the skill
//! that carries a player to its lifestone) plays the recall, then takes the player through portal
//! space to a place beside the lifestone, facing it.
//!
//! **Kept places.** A character keeps where its recall lands and where the last portal it used
//! that may be recalled to took it (the portal's flags, [`yalain_content::portal`]; 2017: Portal
//! Recall's landing was that portal's place 156 of 156 times, past the portals that may not be,
//! the summoned ones and every other trip; Summon Portal's portal led there 34 of 34).

use auberean_primitives::Vec3;
use auberean_protocol::physics::{Fx, MoveOrder, ObjectEvent, Word};
use auberean_protocol::types::{
    BehaviorParams, MoveTarget, MovementParams, ObjectStamp, Placement,
};
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::{NativeObject, TextInfo};
use dereth_primitives::num::math;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::calls::{Node, Value};
use crate::graph::{Builder, Slot};
use crate::world::{heading_of_rotation, Player, ARRIVAL_FX, ON_GROUND};
use crate::{presence, session, Clock, Server, Timer};
use yalain_content::quests::Trigger;

/// The kinds of places a character keeps.
pub mod kind {
    /// Where its lifestone recall takes it.
    pub const LIFESTONE: u32 = 0x4000_0001;
    /// Where the last portal it used that may be recalled to took it (Portal Recall's place).
    pub const LAST_PORTAL: u32 = 0x4000_0004;
}

/// The client functions places answer with.
pub mod function {
    pub const ENTER_PORTAL_SPACE: u32 = 0x8212_0020;
    pub const EXIT_PORTAL_SPACE: u32 = 0x8212_001B;
    /// A door's scene played in place of portal space, and its end.
    pub const ENTER_PORTAL_SCENE: u32 = 0x8212_0064;
    pub const EXIT_PORTAL_SCENE: u32 = 0x8212_0062;
    pub const ANIMATION_FROZEN: u32 = 0x8212_001F;
    /// The answer to a use.
    pub const USE_DONE: u32 = 0x8216_0021;
    /// The player's attack state (recalling counts as attacking).
    pub const ATTACK_STATE: u32 = 0x8204_001B;
}

/// Effects of a trip.
pub mod fx {
    /// A portal flashes as it is used.
    pub const PORTAL_USED: u32 = 0x4100_0044;
    /// The player's teleport.
    pub const TELEPORT: u32 = 0x4100_04FA;
}

/// Behaviors and modes of binding and recalling.
pub mod behavior {
    pub const LIFESTONE_BINDS: u32 = 0x4100_0086;
    pub const PLAYER_BINDS: u32 = 0x4100_0022;
    pub const RECALLING: u32 = 0x4100_00BF;
    pub const RECALL_MODE: u32 = 0x4100_0011;
    pub const STANDING: u32 = 0x4000_0001;
    /// The player uses a held item on an object.
    pub const USES_ON: u32 = 0x4100_00FC;
}

/// The flags of the use-on behavior beside its behavior and context (0x20 and 0x100, no data).
pub const USE_BEHAVIOR_FLAGS: u32 = 0x120;

/// How long after a held item's use on an object the object does its work, in seconds (2017: 2.50
/// and 2.59 s in the two halls' runs).
pub const OPENS_AFTER: f64 = 2.5;

/// Texts of trips: (string id, table).
pub mod text {
    pub const TELEPORTING: (u32, u32) = (0xADE9_426F, 0x2500_0014);
    pub const BOUND: (u32, u32) = (0x1E2D_6E94, 0x2500_0014);
}

/// Items of the pack whose use does nothing but answer, by the script package they derive from:
/// the answer's status and control flags (2017: motes refused as unusable 11 of 11, a dye plant
/// answered 1 of 1, lodestones answered 6 of 6, each at once with nothing else).
pub const HELD_ANSWERED: [(&str, u32, u32); 3] = [
    ("MoteTemplate", USE_STATUS_IN_USE, 316),
    ("DyePlantTemplate", USE_STATUS_OK, 444),
    ("Lodestone", USE_STATUS_OK, 444),
];

/// The teaching stones' script package (the training hall's signs: "Movement", "Quest Panel", ...),
/// the script library's class every teaching stone derives from (the training hall's, a vault's
/// door's, a map's), and the call that has the client examine an object.
pub const TEACHING_STONE: u32 = 0x1352;
pub const TEACHING_STONE_CLASS: &str = "TeachingStoneTemplate";
/// The script library's class every town's architect derives from.
pub const ARCHITECT_CLASS: &str = "ArchitectTemplate";
pub const FORCE_EXAMINE: u32 = 0x8212_0026;

/// Is a script package a teaching stone's: the training hall's own, or one deriving from it?
pub fn is_teaching_stone<S: Store>(s: &Server<S>, package: u32) -> bool {
    package == TEACHING_STONE || s.teaching_stone_packages.contains(&package)
}

/// The script library's lifestone: whatever derives from it is one (2017 answered a use of the
/// human's, the Lugian's, the Tumerok's and the plain one alike: bound, with the same text and
/// behaviors).
pub const LIFESTONE: &str = "Lifestone";

/// Is an object of this script package a lifestone?
#[must_use]
pub fn is_lifestone<S: Store>(s: &Server<S>, package: u32) -> bool {
    s.lifestone_packages.contains(&package)
}

/// How close a player must be to use a portal and a lifestone, in meters.
pub const PORTAL_REACH: f32 = 5.0;
pub const LIFESTONE_REACH: f32 = 2.0;

/// How long a player stays in portal space, in seconds.
pub const IN_PORTAL_SPACE: f64 = 1.5;

/// How far from its lifestone a recalled player lands, in meters.
pub const BESIDE_LIFESTONE: f32 = 4.5;

/// The answer to a use: its status, control flags, and the user's kind.
pub const USE_STATUS_OK: u32 = 0;
/// A use answered while the player's earlier action is still being carried out (in portal space,
/// dead, walking to an earlier use's object, taking an item).
pub const ACTIONS_LOCKED: u32 = 1029;
pub const USE_FLAGS_TELEPORTED: u32 = 948;
pub const USE_FLAGS_BOUND: u32 = 820;
/// The answer's control flags for an item used on an object that took the player somewhere.
pub const USE_FLAGS_UNLOCKED: u32 = 317;
/// The answer's control flags for a portal its quest keeps the player out of (2017: 2 of 2).
pub const USE_FLAGS_QUEST_KEEPS_OUT: u32 = 820;

/// The answer's control flags for an item lying in the world its quest keeps the player from
/// (2017: 1 of 1).
pub const USE_FLAGS_TAKE_REFUSED: u32 = 828;

/// The answer's control flags for a person or an object used (a storekeeper, a quest giver, a
/// stone): every such use in 2017 answered with them.
pub const USE_FLAGS_PERSON: u32 = 956;

/// How close a player must be to use a person, in meters.
pub const PERSON_REACH: f32 = 5.0;

/// How far beyond a walk's range a walking player may be from the object's body ([`walk_gap`]) for
/// the walk to be done (2017's servers ended walks to people with the centers a median 1.0 m
/// beyond the range, 3,787 walks), and a player that has come to rest (authored, wide enough for
/// a step and a report's lag).
pub const WALK_SLACK: f32 = 1.0;
pub const STILL_SLACK: f32 = 4.0;
pub const USER_IS_PLAYER: u32 = 31;

/// The body a walk's end measures from: the walking player's center, at a player's height.
const WALK_FROM: crate::reach::Body = crate::reach::Body {
    radius: 0.0,
    height: crate::reach::PLAYER.height,
};

/// How far a walking player is from an object of a template (at its own scale, if it has one), as
/// its client ends a walk to it: from the player's center to the side of the object's upright body
/// (its look's radius and height) across the ground, and past the lower one's top in height; the
/// distance between their centers where the template's body is not known. (2017: the servers ended
/// walks to people, lifestones and doors with the player's last report a median 0.48, 0.35 and
/// 1.32 m inside the walk's range of the object's side, 609, 247 and 137 walks; the Tumerok's
/// lifestone, 4.46 m wide, was used from 3.0 to 12.2 m off its center, 61 of 105 walked uses
/// farther than 6 m.)
pub(crate) fn walk_gap<S: Store>(
    s: &Server<S>,
    at: &StoredPosition,
    template: u32,
    scale: Option<f32>,
    cell: u32,
    origin: [f32; 3],
) -> Option<f32> {
    let Some([r, h]) = s.catalog.bodies.get(&template) else {
        return reach(at, cell, origin);
    };
    let scale = scale.unwrap_or(1.0);
    let body = crate::reach::Body {
        radius: r * scale,
        height: h * scale,
    };
    let there = StoredPosition {
        cell,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    crate::reach::gap(at, WALK_FROM, &there, body)
}

/// The slots of a use's description, by index (its package's layout).
pub mod usage {
    pub const PACKAGE: u16 = 0x245;
    pub const SLOTS: usize = 20;
    pub const MESSAGE: usize = 0;
    pub const ITEM: usize = 2;
    /// The object a held item is used on (two words).
    pub const TARGET: usize = 9;
    pub const USER_PLACE: usize = 4;
    pub const USER_KIND: usize = 5;
    pub const DISTANCE: usize = 8;
    pub const STATUS: usize = 11;
    pub const EFFECTIVE_TARGET: usize = 12;
    /// The kind of target the use was made on.
    pub const TARGET_KIND: usize = 14;
    pub const FLAGS: usize = 17;
    pub const CANCELS: usize = 18;
}

/// A step of a trip through portal space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// An item used on an object has done its work (a glyph has opened a vault): the object
    /// flashes, into portal space.
    Opened,
    /// Out of portal space, at the destination.
    Arrive,
    /// Out of a door's scene, at the destination.
    OutOfScene { scene: u32 },
    /// A story shard's wait is over: its scene, then into portal space.
    Shard { scene: u32 },
    /// A use of this person or object has moved a quest that leads on: it flashes, into portal
    /// space.
    Sent { by: u64 },
}

/// A use of an object the server is carrying out: the request's description (its slots and its
/// message), the object, and where the player stood as it asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Use {
    pub item: u64,
    /// The object the item is used on (0: none).
    pub target: u64,
    pub slots: Vec<(u8, u32)>,
    pub message: Option<TextInfo>,
    pub from: StoredPosition,
}

/// A trip under way: where to, the use that started it (none for a recall), and the quest step
/// it makes on arrival.
#[derive(Debug, Clone, PartialEq)]
pub struct Trip {
    pub to: StoredPosition,
    pub used: Option<Use>,
    pub step: Option<yalain_content::quests::Step>,
    /// A dead player's return to life.
    pub resurrect: bool,
    /// Sent by an effect: no word of it on arrival.
    pub sent: bool,
    /// A recall's text on arrival ("You recall to your lifestone."), if it says one.
    pub said: Option<TextInfo>,
    /// The quest updates told as it comes out, each its quest and the kind of update: an update
    /// whose own recall sent the player.
    pub updates: Vec<(u32, u32)>,
}

fn send(net: &mut ServerNet, conn: ConnId, kind: BlobKind, m: &ServerMessage) {
    net.send(conn, queue::EVENT, kind, m.encode());
}

/// Send a visual event about a player to it and every client that knows it.
pub(crate) fn to_all_knowing<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    id: u64,
    m: &ServerMessage,
) {
    let bytes = m.encode();
    for (conn, q) in &s.players {
        if q.character == id || q.known.contains_key(&id) {
            net.send(*conn, queue::EVENT, BlobKind::VISUAL, bytes.clone());
        }
    }
}

/// Send a visual event about a content object to every client that knows it.
pub(crate) fn to_knowers<S: Store>(s: &Server<S>, net: &mut ServerNet, id: u64, m: &ServerMessage) {
    let bytes = m.encode();
    for (conn, q) in &s.players {
        if q.known.contains_key(&id) {
            net.send(*conn, queue::EVENT, BlobKind::VISUAL, bytes.clone());
        }
    }
}

/// The next visual stamp of a content object (first made with stamp 1, then with its latest).
pub fn object_stamp<S: Store>(s: &mut Server<S>, id: u64) -> ObjectStamp {
    let instance_stamp = s.placed.instance(id);
    let v = s.visuals.entry(id).or_insert(1);
    *v = v.wrapping_add(1);
    ObjectStamp {
        id,
        instance_stamp,
        other_stamp: *v,
    }
}

pub(crate) fn player_event<A>(p: &mut Player, action: A) -> ObjectEvent<A> {
    p.visual = p.visual.wrapping_add(1);
    ObjectEvent {
        sender: ObjectStamp {
            other_stamp: p.visual,
            ..p.stamp()
        },
        action,
    }
}

/// The client told to show an object's examination.
pub(crate) fn force_examine(id: u64) -> ServerMessage {
    call(FORCE_EXAMINE, &[Arg::LongInt(id)])
}

fn call(f: u32, args: &[Arg]) -> ServerMessage {
    session::call(f, args).expect("plain values pack")
}

fn flag(f: u32, on: bool) -> ServerMessage {
    call(f, &[Arg::Int(u32::from(on))])
}

/// Freeze or free the player's animation.
pub(crate) fn animation_frozen(on: bool) -> ServerMessage {
    flag(function::ANIMATION_FROZEN, on)
}

/// Entering or leaving portal space, with no delay.
fn portal_space(f: u32) -> ServerMessage {
    call(f, &[Arg::LongFloat(0f64.to_bits())])
}

/// The call that puts a player's client into portal space.
pub(crate) fn into_portal_space() -> ServerMessage {
    portal_space(function::ENTER_PORTAL_SPACE)
}

/// A door's scene begun or ended (the scene whole, with no delay).
fn portal_scene<S: Store>(s: &Server<S>, f: u32, scene: u32) -> ServerMessage {
    let scenes = s
        .story
        .scenes
        .get(&scene)
        .map(crate::scenes::scene_info)
        .into_iter()
        .collect();
    let mut b = Builder::new();
    b.native(crate::scenes::SCENE_LIST, NativeObject::Scenes(scenes));
    call(f, &[Arg::Object(b.graph()), Arg::LongFloat(0f64.to_bits())])
}

/// The use answer's status for a corpse the player has no right to yet.
pub const USE_STATUS_NO_RIGHT: u32 = 10000;

/// The use answer's status and control flags for a corpse another player has open.
pub const USE_STATUS_IN_USE: u32 = 10001;
pub const USE_FLAGS_IN_USE: u32 = 956;

/// The use answer's control flags for an item of the pack with no use, refused.
pub const USE_FLAGS_PACK: u32 = 316;

/// The use answer's status for an item on the user's own trade offer: "That item is on your trade
/// offer list."
pub const USE_STATUS_ON_TRADE_OFFER: u32 = 10_039;

/// The use answer's control flags for another player used: it cannot be (the status is the same
/// "cannot use").
pub const USE_FLAGS_A_PLAYER: u32 = 828;

/// The use answer's control flags for a creature used (an attack begun).
pub const USE_FLAGS_ATTACK: u32 = 928;

/// Stop a player walking to a use (it has arrived).
pub(crate) fn stop_walking<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.walking_to = None;
    let stop = stop_order(p);
    let id = p.character;
    to_all_knowing(s, net, id, &stop);
}

/// A player using a corpse it has no right to is refused where it stands: is this that?
fn corpse_refused<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    clock: Clock,
) -> bool {
    let Some(at) = s
        .things
        .get(u.item)
        .filter(|t| t.kind == crate::things::Kind::Corpse)
        .and_then(|t| t.at)
    else {
        return false;
    };
    let me = s.players.get(&world).map_or(0, |p| p.character);
    if crate::death::may_loot(s, me, u.item, clock.local.0) {
        return false;
    }
    let distance = presence::distance(&u.from, &at).unwrap_or(0.0);
    let no = use_answer(
        u,
        me,
        Some(USE_FLAGS_IN_USE),
        false,
        distance,
        USE_STATUS_NO_RIGHT,
    );
    send(net, world, BlobKind::PRIVATE, &no);
    true
}

/// A player uses a thing of the world (in reach where reach matters).
fn use_thing<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let Some(t) = s.things.get(u.item) else {
        return;
    };
    let me = s.players.get(&world).map_or(0, |p| p.character);
    if corpse_refused(s, net, world, &u, clock) {
        return;
    }
    match t.kind {
        // One viewer at a time: a corpse another player has open refuses the use.
        crate::things::Kind::Corpse if crate::inventory::viewed_by_another(s, world, u.item) => {
            let distance = s
                .things
                .get(u.item)
                .and_then(|t| t.at)
                .and_then(|at| presence::distance(&u.from, &at))
                .unwrap_or(0.0);
            let no = use_answer(
                &u,
                me,
                Some(USE_FLAGS_IN_USE),
                false,
                distance,
                USE_STATUS_IN_USE,
            );
            send(net, world, BlobKind::PRIVATE, &no);
        }
        crate::things::Kind::Corpse => crate::inventory::open(s, net, world, u.item),
        crate::things::Kind::Item => match t.container {
            Some(c) if crate::inventory::open_corpse_of(s, world) == Some(c) => {
                // A take its quest keeps from the player is refused at once.
                if let Some(why) = take_refused(s, world, t.template, clock) {
                    let no = use_refused(&u, 0, None, -1.0, why);
                    send(net, world, BlobKind::PRIVATE, &no);
                    return;
                }
                crate::inventory::take(s, net, world, u.item, c, clock);
            }
            Some(c) if c == me => use_held(s, net, world, u, clock),
            _ => s.unhandled.note("using an item out of reach"),
        },
        crate::things::Kind::Node => crate::mining::begin(s, net, world, u, clock),
        crate::things::Kind::Portal => crate::usable::portals::at_portal(s, net, world, u, clock),
        crate::things::Kind::Plant => crate::plants::begin(s, net, world, u, clock),
        // A chest one placed is opened as any chest; anything else it placed is no one's to use.
        crate::things::Kind::Placed if crate::usable::chests::chest(s, u.item).is_some() => {
            crate::usable::chests::at_chest(s, net, world, u, clock);
        }
        crate::things::Kind::Placed => {
            let no = use_refused(&u, me, None, 0.0, USE_STATUS_OK);
            send(net, world, BlobKind::PRIVATE, &no);
        }
        crate::things::Kind::Creature => {
            let distance = s
                .things
                .get(u.item)
                .and_then(|t| t.at)
                .and_then(|at| presence::distance(&u.from, &at))
                .unwrap_or(0.0);
            // A player's own escort is a person used, its use running the steps it triggers.
            let escort = s
                .things
                .get(u.item)
                .filter(|_| s.escorts.follows(u.item, me))
                .map(|t| (t.template, t.description.name));
            if let Some((template, name)) = escort {
                let done = use_done(&u, me, USE_FLAGS_PERSON, false, distance);
                send(net, world, BlobKind::PRIVATE, &done);
                let who = Some((u.item, name));
                crate::quests::triggered(s, net, world, Trigger::Use(template), who, clock);
                return;
            }
            let done = use_done(&u, me, USE_FLAGS_ATTACK, false, distance);
            send(net, world, BlobKind::PRIVATE, &done);
            crate::combat::begin(s, net, world, u.item, clock);
        }
    }
}

/// Is this item in the player's pack, neither worn nor wielded?
fn in_the_pack<S: Store>(s: &Server<S>, world: ConnId, item: u64) -> bool {
    s.players
        .get(&world)
        .is_some_and(|p| p.play.items.iter().any(|h| h.id == item && h.location == 0))
}

/// Does an item template name a quest its use needs?
fn names_a_use_quest<S: Store>(s: &Server<S>, template: u32) -> bool {
    s.catalog
        .get(template)
        .is_some_and(|t| t.props.word("Usage_RequiredQuest").is_some())
}

/// Why taking an item of a template is refused, if its quest keeps the player from it.
pub(crate) fn take_refused<S: Store>(
    s: &Server<S>,
    world: ConnId,
    template: u32,
    clock: Clock,
) -> Option<u32> {
    let props = &s.catalog.get(template)?.props;
    crate::item_use::take_gate(s, world, props, clock)
}

/// Why using an object of the world (a portal) is refused, if its quest keeps the player out: the
/// quest its placed instance needs, else its template's (2017: the generic portals, whose
/// templates need none, refused 40 times at 13 placed instances).
pub(crate) fn quest_keeps_out<S: Store>(
    s: &Server<S>,
    world: ConnId,
    o: &yalain_content::ContentObject,
    clock: Clock,
) -> Option<u32> {
    if let Some(why) = under_its_level(s, world, o) {
        return Some(why);
    }
    if let Some(why) = not_who_it_is_for(s, world, o) {
        return Some(why);
    }
    if let Some(g) = o.gate {
        return crate::item_use::needs_quest(s, world, g.quest, g.status, clock);
    }
    let props = &s.catalog.get(o.template)?.props;
    crate::item_use::quest_gate(s, world, props, clock)
}

/// The refusal of a player below a placed object's least level, the one its examination names.
fn under_its_level<S: Store>(
    s: &Server<S>,
    world: ConnId,
    o: &yalain_content::ContentObject,
) -> Option<u32> {
    let least = o.facts.min_level?;
    let p = s.players.get(&world)?;
    (crate::examine::level_of(&p.qualities) < least)
        .then_some(crate::item_use::refusal::LEVEL_TOO_LOW)
}

/// The refusal of a player a placed object is not for: of another kingdom than the one it names, or
/// no hero when it is for heroes alone, as its examination says ([`for_heroes`]).
fn not_who_it_is_for<S: Store>(
    s: &Server<S>,
    world: ConnId,
    o: &yalain_content::ContentObject,
) -> Option<u32> {
    let p = s.players.get(&world)?;
    if o.facts
        .kingdom
        .is_some_and(|k| crate::kingdoms::of(&p.qualities) != Some(k))
    {
        return Some(crate::item_use::refusal::WRONG_KINGDOM);
    }
    (for_heroes(s, o) && !crate::focus::is_hero(&p.qualities))
        .then_some(crate::item_use::refusal::HERO_ONLY)
}

/// Is a placed object for heroes alone: its row says so, or its template does?
fn for_heroes<S: Store>(s: &Server<S>, o: &yalain_content::ContentObject) -> bool {
    o.facts.hero
        || s.catalog
            .get(o.template)
            .is_some_and(|t| t.props.flag("Usage_HeroOnly"))
}

/// Does a placed object keep some players out by what its own row or template says: a quest, a
/// kingdom, heroes?
fn keeps_players_out<S: Store>(s: &Server<S>, o: &yalain_content::ContentObject) -> bool {
    o.gate.is_some() || for_heroes(s, o) || o.facts.kingdom.is_some()
}

/// Is a template one a player talks to by using it (a storekeeper, a person or object a quest
/// step is triggered by or an order of uses names, or one that says something when used)?
fn is_person<S: Store>(s: &Server<S>, template: u32) -> bool {
    s.stores.keeper(template).is_some()
        || crate::books::npc_book(s, template).is_some()
        || s.item_rows.in_a_sequence(template)
        || s.quests
            .steps
            .iter()
            .any(|st| st.trigger == yalain_content::quests::Trigger::Use(template))
        || (crate::talk::talks_when_used(s, template)
            && !s
                .catalog
                .get(template)
                .is_some_and(|t| is_lifestone(s, t.description.package)))
}

/// Is a content object one a player talks to: a person, never a portal. A placed person or object
/// of the script library's person kind with nothing else to do is one, saying nothing of its own
/// (2017: 78 of 78 uses of such objects answered as a person's, 12 of them with nothing said).
fn talks_to<S: Store>(s: &Server<S>, o: &yalain_content::ContentObject) -> bool {
    let person = is_person(s, o.template)
        || (o.destination.is_none() && s.person_packages.contains(&o.description.package));
    person && (o.destination.is_none() || !crate::talk::talks_when_used(s, o.template))
}

/// The script library's kinds whose placed objects answer a use as a person does.
pub const PERSONS: [&str; 1] = ["NPC"];

/// The script library's kinds whose placed objects cannot be used: forges and road signs, refused
/// at once wherever the player stands.
pub const CANNOT_BE_USED: [&str; 2] = ["Forge", "RoadSignTemplate"];

/// The use answer's control flags for a placed object that cannot be used.
pub const USE_FLAGS_CANNOT_BE_USED: u32 = 828;

fn p_id<S: Store>(s: &Server<S>, world: ConnId) -> u64 {
    s.players.get(&world).map_or(0, |p| p.character)
}

/// The distance from a place to an object's.
pub(crate) fn reach(a: &StoredPosition, cell: u32, origin: [f32; 3]) -> Option<f32> {
    presence::distance(
        a,
        &StoredPosition {
            cell,
            origin,
            rotation: [1.0, 0.0, 0.0, 0.0],
        },
    )
}

/// Read a use request: the object, the description's slots and message.
#[must_use]
pub fn read_use(v: &[Value], from: StoredPosition) -> Option<Use> {
    let Value::Graph(g) = v.first()? else {
        return None;
    };
    let (_, Node::Script { package, slots }) = g.first()? else {
        return None;
    };
    if *package != usage::PACKAGE || slots.len() != usage::SLOTS {
        return None;
    }
    let item = u64::from(slots[usage::ITEM + 1].1) << 32 | u64::from(slots[usage::ITEM].1);
    let target = u64::from(slots[usage::TARGET + 1].1) << 32 | u64::from(slots[usage::TARGET].1);
    let message = g.iter().find_map(|(_, n)| match n {
        Node::TextInfo(t) => Some(t.clone()),
        _ => None,
    });
    Some(Use {
        item,
        target,
        slots: slots.clone(),
        message,
        from,
    })
}

/// The answer to a use: the request's description with the outcome filled in (status, control
/// flags, the user's kind, place and distance, the effective target, whether it cancels).
#[must_use]
pub fn use_done(u: &Use, player: u64, flags: u32, cancels: bool, distance: f32) -> ServerMessage {
    use_answer(u, player, Some(flags), cancels, distance, USE_STATUS_OK)
}

/// The answer to a use with its status (the request's control flags kept when none are given).
fn use_answer(
    u: &Use,
    player: u64,
    flags: Option<u32>,
    cancels: bool,
    distance: f32,
    status: u32,
) -> ServerMessage {
    answer(u, player, None, flags, cancels, distance, status)
}

/// A use refused: its status, its effective target, its control flags (the request's kept when
/// none are given).
pub(crate) fn use_refused(
    u: &Use,
    effective: u64,
    flags: Option<u32>,
    distance: f32,
    status: u32,
) -> ServerMessage {
    use_answer(u, effective, flags, false, distance, status)
}

/// The answer to a use made on an object: the request's control flags kept, its effective target
/// and the kind of target it was made on given (a use of a held item on an object of the world).
#[must_use]
pub fn use_answer_to(
    u: &Use,
    _player: u64,
    effective: u64,
    target_kind: u32,
    distance: f32,
    status: u32,
) -> ServerMessage {
    answer(
        u,
        effective,
        Some(target_kind),
        None,
        false,
        distance,
        status,
    )
}

/// The answer to a held item used on an object at once: its control flags, the object's kind of
/// target named, no effective target.
pub(crate) fn use_answer_on(u: &Use, flags: u32, distance: f32, status: u32) -> ServerMessage {
    answer(
        u,
        0,
        Some(crate::usable::interactions::ON_AN_OBJECT),
        Some(flags),
        false,
        distance,
        status,
    )
}

fn answer(
    u: &Use,
    player: u64,
    target_kind: Option<u32>,
    flags: Option<u32>,
    cancels: bool,
    distance: f32,
    status: u32,
) -> ServerMessage {
    let mut b = Builder::new();
    let root = b.reserve(usage::PACKAGE);
    let message = u
        .message
        .clone()
        .map(|t| b.native(session::native::TEXT, NativeObject::TextInfo(t)));
    let f = &u.from;
    let place = b.native(
        90,
        NativeObject::Position {
            cell: f.cell,
            frame: [
                f.origin[0].to_bits(),
                f.origin[1].to_bits(),
                f.origin[2].to_bits(),
                f.rotation[0].to_bits(),
                f.rotation[1].to_bits(),
                f.rotation[2].to_bits(),
                f.rotation[3].to_bits(),
            ],
        },
    );
    let mut slots: Vec<Slot> = u.slots.iter().map(|(t, v)| Slot::Tagged(*t, *v)).collect();
    let set = |slots: &mut Vec<Slot>, i: usize, v: u32| {
        if let Slot::Tagged(t, _) = slots[i] {
            slots[i] = Slot::Tagged(t, v);
        }
    };
    slots[usage::MESSAGE] = Slot::Ref(message);
    slots[usage::USER_PLACE] = Slot::Ref(Some(place));
    set(&mut slots, usage::USER_KIND, USER_IS_PLAYER);
    set(&mut slots, usage::DISTANCE, distance.to_bits());
    set(&mut slots, usage::STATUS, status);
    #[allow(clippy::cast_possible_truncation)]
    {
        slots[usage::EFFECTIVE_TARGET] = Slot::Tagged(0x82, player as u32);
        slots[usage::EFFECTIVE_TARGET + 1] = Slot::Tagged(0xCD, (player >> 32) as u32);
    }
    if let Some(flags) = flags {
        set(&mut slots, usage::FLAGS, flags);
    }
    set(&mut slots, usage::CANCELS, u32::from(cancels));
    if let Some(k) = target_kind {
        set(&mut slots, usage::TARGET_KIND, k);
    }
    b.fill(root, &slots);
    call(function::USE_DONE, &[Arg::Object(b.graph())])
}

/// Is a player's earlier action still being carried out, so that a use is refused: in portal
/// space, dead, walking to an earlier use's object, taking an item (or everything), turning a
/// corpse's items into money, or using one?
#[must_use]
pub fn actions_locked<S: Store>(s: &Server<S>, world: ConnId) -> bool {
    let Some(p) = s.players.get(&world) else {
        return false;
    };
    p.trip.is_some()
        || p.dying.is_some()
        || crate::combat::is_dead(p)
        || p.walking_to.is_some()
        || crate::usable::doors::waiting_on(s, world)
        || crate::usable::chests::waiting_on(s, world)
        || s.timers.iter().any(|(_, t)| {
            matches!(t, Timer::Take { world: w, .. }
                | Timer::TakeAll { world: w, .. }
                | Timer::Transmute { world: w, .. }
                | Timer::PickUp { world: w, .. }
                | Timer::Butcher { world: w, .. }
                | Timer::Mine { world: w, .. }
                | Timer::QuestMine { world: w, .. }
                | Timer::ItemUse { world: w, .. }
                | Timer::Harvest { world: w, .. }
                | Timer::Interaction { world: w, .. } if *w == world)
        })
}

/// Does a use under way count on what the player's pack holds until it is done: a trip (a glyph or
/// a step's item goes as it arrives), a key turning, a knife cutting? Nothing leaves the pack by the
/// player's asking until then: not put down, not sold, not traded.
#[must_use]
pub fn pack_spoken_for<S: Store>(s: &Server<S>, world: ConnId) -> bool {
    s.players.get(&world).is_some_and(|p| p.trip.is_some())
        || crate::usable::doors::waiting_on(s, world)
        || crate::usable::chests::waiting_on(s, world)
        || s.timers
            .iter()
            .any(|(_, t)| matches!(t, Timer::Butcher { world: w, .. } if *w == world))
}

/// A player asks to use an object.
pub fn use_object<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let Some(u) = read_use(v, p.position) else {
        s.unhandled.note("a use that does not read");
        return;
    };
    if actions_locked(s, world) {
        let me = p_id(s, world);
        let refused = use_answer(&u, me, None, false, 0.0, ACTIONS_LOCKED);
        send(net, world, BlobKind::PRIVATE, &refused);
        return;
    }
    // An item on its own trade offer is refused at once (2017: 2 of 2 such uses).
    if s.trades.on_offer(p.character, u.item) {
        let no = use_refused(&u, 0, Some(USE_FLAGS_PACK), -1.0, USE_STATUS_ON_TRADE_OFFER);
        send(net, world, BlobKind::PRIVATE, &no);
        return;
    }
    // A chest the world made: walked to, then opened, as a placed one is.
    if s.things.get(u.item).is_some() && crate::usable::chests::chest(s, u.item).is_some() {
        crate::usable::chests::used(s, net, world, u, clock);
        return;
    }
    if let Some(t) = s.things.get(u.item) {
        // An item lying in the world: walked to (the order sent even when the player is there),
        // then picked up.
        if t.kind == crate::things::Kind::Item && t.container.is_none() && t.at.is_some() {
            // One its quest keeps from the player is refused at once, where it stands.
            if let Some(why) = take_refused(s, world, t.template, clock) {
                let no = use_refused(&u, 0, Some(USE_FLAGS_TAKE_REFUSED), -1.0, why);
                send(net, world, BlobKind::PRIVATE, &no);
                return;
            }
            walk_to_with(
                s,
                net,
                world,
                u,
                crate::inventory::PICK_UP_REACH,
                PICK_UP_WALK,
            );
            arrived(s, net, world, clock);
            // Only the client's reports after the order say whether it could get there.
            if let Some(p) = s.players.get_mut(&world) {
                p.walk_rest = None;
            }
            return;
        }
        // A resource node: walked to, then mined.
        if let (Some(at), crate::things::Kind::Node) = (t.at, t.kind) {
            if presence::distance(&p.position, &at).is_some_and(|d| d <= crate::mining::REACH) {
                crate::mining::begin(s, net, world, u, clock);
            } else {
                walk_to(s, net, world, u, crate::mining::REACH);
            }
            return;
        }
        // A portal a player summoned: walked to, then through it.
        if t.kind == crate::things::Kind::Portal {
            crate::usable::portals::used(s, net, world, u, clock);
            return;
        }
        // A dye plant: walked to, then harvested.
        if t.kind == crate::things::Kind::Plant {
            crate::plants::used(s, net, world, u, clock);
            return;
        }
        // A thing of the world: a corpse to open (walked to it first), an item to take, a creature
        // to attack.
        if let (Some(at), crate::things::Kind::Corpse) = (t.at, t.kind) {
            if corpse_refused(s, net, world, &u, clock) {
                return;
            }
            if presence::distance(&p.position, &at)
                .is_some_and(|d| d <= crate::inventory::CORPSE_REACH)
            {
                use_thing(s, net, world, u, clock);
            } else {
                walk_to(s, net, world, u, crate::inventory::CORPSE_REACH);
            }
            return;
        }
        use_thing(s, net, world, u, clock);
        return;
    }
    // Another player cannot be used: refused at once where the user stands, with the distance
    // between them, and the other told nothing.
    let other = presence::player_for(s, world, u.item).and_then(|w| s.players.get(&w));
    if let Some(other) = other {
        let distance = presence::distance(&p.position, &other.position).unwrap_or(0.0);
        let no = use_refused(&u, 0, Some(USE_FLAGS_A_PLAYER), distance, USE_STATUS_IN_USE);
        send(net, world, BlobKind::PRIVATE, &no);
        return;
    }
    let Some(i) = crate::events::content_standing(s, u.item) else {
        s.unhandled.note("using an object the world does not have");
        return;
    };
    let o = &s.content.objects[i];
    if is_teaching_stone(s, o.description.package) {
        // A teaching stone shows its page: the client is told to examine it, and the use is
        // answered, wherever the player stands (2017: 14 of 15 uses, no walk; the Arwic vault's
        // stone, of a class of its own, 1 of 1).
        let me = p_id(s, world);
        let examine = call(FORCE_EXAMINE, &[Arg::LongInt(u.item)]);
        send(net, world, BlobKind::PRIVATE, &examine);
        let done = use_done(
            &u,
            me,
            USE_FLAGS_PERSON,
            false,
            reach(&u.from, o.cell, o.origin).unwrap_or(0.0),
        );
        send(net, world, BlobKind::PRIVATE, &done);
        return;
    }
    if crate::usable::levers::is_lever(s, u.item) {
        crate::usable::levers::used(s, net, world, u, clock);
        return;
    }
    if crate::usable::doors::door(s, u.item).is_some() {
        crate::usable::doors::used(s, net, world, u, clock);
        return;
    }
    if crate::usable::chests::chest(s, u.item).is_some() {
        crate::usable::chests::used(s, net, world, u, clock);
        return;
    }
    if crate::usable::vaults::vault(s, u.item).is_some() {
        crate::usable::vaults::needs_glyph(s, net, world, &u, u.item);
        return;
    }
    if s.shrine_packages.contains(&o.description.package) {
        crate::usable::shrines::used(s, net, world, u, clock);
        return;
    }
    // A mine a quest or a recipe works: walked to within reach, then worked or refused.
    if crate::mining::quest_mine(s, o.template).is_some() {
        if reach(&p.position, o.cell, o.origin).is_some_and(|d| d <= crate::mining::REACH) {
            let template = o.template;
            crate::mining::at_quest_mine(s, net, world, u, template, clock);
        } else {
            walk_to(s, net, world, u, crate::mining::REACH);
            arrived(s, net, world, clock);
        }
        return;
    }
    let lifestone = is_lifestone(s, o.description.package);
    let person = talks_to(s, o);
    let broker = crate::market::broker(s, o.template).is_some();
    // An object a hazard harms at a use is used as a person is, walked to and answered.
    let hazard = o.destination.is_none() && crate::hazards::harms_at_use(s, i);
    // An object for monarchs alone is refused at once, wherever the player stands, whatever else it
    // asks of the player (2017: the Allegiance Sign and the Binding Stone, 2 of 2, at 17.9 and
    // 5.7 m, the stone's quest not held).
    if s.catalog
        .get(o.template)
        .is_some_and(|t| crate::item_use::not_monarch(s, world, &t.props))
    {
        let distance = reach(&u.from, o.cell, o.origin).unwrap_or(0.0);
        let no = use_refused(
            &u,
            0,
            Some(USE_FLAGS_CANNOT_BE_USED),
            distance,
            crate::item_use::refusal::MONARCH_ONLY,
        );
        send(net, world, BlobKind::PRIVATE, &no);
        return;
    }
    // A hall's binding stone: a monarch is walked to it, then binds its allegiance there.
    if crate::usable::halls::stone(s, u.item).is_some() {
        crate::usable::halls::used(s, net, world, u, clock);
        return;
    }
    // A portal its quest keeps the player out of is refused at once, wherever the player stands
    // (2017: 2 of 2, at 13.5 and 56.9 m; the generic portals kept by their placed instance's
    // quest, 21 of 21 requests answered in 0.09-0.46 s, at 2.9-76.0 m), whether or not where it
    // leads is known.
    if (o.destination.is_some() || keeps_players_out(s, o)) && !lifestone && !person && !broker {
        if let Some(why) = quest_keeps_out(s, world, o, clock) {
            let distance = reach(&u.from, o.cell, o.origin).unwrap_or(0.0);
            let no = use_refused(&u, 0, Some(USE_FLAGS_QUEST_KEEPS_OUT), distance, why);
            send(net, world, BlobKind::PRIVATE, &no);
            return;
        }
    }
    if o.destination.is_none() && !lifestone && !person && !broker && !hazard {
        if s.cannot_use_packages.contains(&o.description.package) {
            let distance = reach(&u.from, o.cell, o.origin).unwrap_or(0.0);
            let no = use_refused(
                &u,
                0,
                Some(USE_FLAGS_CANNOT_BE_USED),
                distance,
                USE_STATUS_IN_USE,
            );
            send(net, world, BlobKind::PRIVATE, &no);
            return;
        }
        s.unhandled
            .note(format!("using an object of template {:08X}", o.template));
        return;
    }
    let range = if lifestone {
        LIFESTONE_REACH
    } else if broker {
        crate::market::BROKER_REACH
    } else if person || hazard {
        PERSON_REACH
    } else {
        PORTAL_REACH
    };
    if reach(&p.position, o.cell, o.origin).is_some_and(|d| d <= range) {
        carry_out(s, net, world, u, false, clock);
    } else {
        walk_to(s, net, world, u, range);
        // A player at rest whose client already counts it in reach reports nothing more: its walk
        // is over as it starts.
        arrived(s, net, world, clock);
    }
}

/// The flags of a movement order that stops the player where it is.
pub(crate) const STOP_FLAGS: u32 = 0x40;

/// A movement order for a player under its last order's stamp; anything but a stop holds until
/// another order replaces it.
pub(crate) fn move_order(p: &mut Player, params: MovementParams) -> ServerMessage {
    p.move_in_force = params.flags != STOP_FLAGS;
    let stamp = p.move_to_stamp;
    ServerMessage::MoveTo(player_event(p, MoveOrder { params, stamp }))
}

/// A movement order stopping the player where it is.
pub(crate) fn stop_order(p: &mut Player) -> ServerMessage {
    move_order(
        p,
        MovementParams {
            flags: STOP_FLAGS,
            desired_velocity: 0.0,
            fail_distance: 0.0,
            desired_distance: 0.0,
            max_success_distance: 0.0,
            min_success_distance: 0.0,
            target: MoveTarget::Place(Placement::default()),
            offset: Vec3::ZERO,
            offset_id: 0,
            waypoints: Vec::new(),
            context: 0,
        },
    )
}

/// What a walk for a use goes to: the object an item is used on, else the object used.
fn walk_goal(u: &Use) -> u64 {
    if u.target == 0 {
        u.item
    } else {
        u.target
    }
}

/// A player uses an item it holds: a quest step it triggers (on the object it is used on, if any),
/// else the item is wielded or put away.
fn use_held<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, u: Use, clock: Clock) {
    if crate::butchery::is_butchery(s, &u) {
        walk_to(s, net, world, u, crate::butchery::REACH);
        arrived(s, net, world, clock);
        return;
    }
    let Some(item) = s.things.get(u.item).map(|t| t.template) else {
        return;
    };
    // An item in the pack that its quest keeps from the player is used no more than taken (2017, pack
    // uses refused 10042, 10021: 13 of 15 of items whose classes name a quest for taking, 8 of 8 of
    // 10042, none of them naming a quest for their use), unless its template names a quest for its
    // use: that one judges its use, as it does any held item's.
    if u.target == 0 && in_the_pack(s, world, u.item) && !names_a_use_quest(s, item) {
        if let Some(why) = take_refused(s, world, item, clock) {
            let no = use_refused(&u, 0, Some(USE_FLAGS_PACK), 0.0, why);
            send(net, world, BlobKind::PRIVATE, &no);
            return;
        }
    }
    if crate::usable::doors::door(s, u.target).is_some() {
        crate::usable::doors::key_used(s, net, world, u, clock);
        return;
    }
    // A contract kept because it could not hire when it was bought tries again.
    if s.pets.is_contract(item) && in_the_pack(s, world, u.item) {
        let hired = crate::pets::hire(s, net, world, item, clock);
        let me = s.players.get(&world).map_or(0, |p| p.character);
        let told = if hired {
            use_done(&u, me, USE_FLAGS_PACK, false, 0.0)
        } else {
            use_refused(&u, 0, Some(USE_FLAGS_PACK), 0.0, USE_STATUS_OK)
        };
        send(net, world, BlobKind::PRIVATE, &told);
        return;
    }
    // A totem used on an item in the pack.
    if s.things
        .get(u.item)
        .is_some_and(|t| s.crafting.crafts.totem_word(t.template).is_some())
    {
        crate::craft::use_totem(s, net, world, &u);
        return;
    }
    if crate::usable::chests::chest(s, u.target).is_some() {
        crate::usable::chests::key_used(s, net, world, u, clock);
        return;
    }
    // An item used on an object its interaction table names, carried out at once.
    if u.target != 0 && crate::usable::interactions::used(s, net, world, &u, clock) {
        return;
    }
    if u.target != 0 {
        let Some(i) = presence::content_by_id(&s.content.objects, u.target) else {
            s.unhandled
                .note("an item used on an object the world does not have");
            let m = use_answer_on(
                &u,
                crate::usable::interactions::FLAGS_KEPT,
                0.0,
                USE_STATUS_OK,
            );
            send(net, world, BlobKind::PRIVATE, &m);
            return;
        };
        let o = &s.content.objects[i];
        let Some(p) = s.players.get(&world) else {
            return;
        };
        if reach(&p.position, o.cell, o.origin).is_some_and(|d| d <= PERSON_REACH) {
            held_on_object(s, net, world, u, clock);
        } else {
            walk_to(s, net, world, u, PERSON_REACH);
        }
        return;
    }
    // An item whose kind has no use from the pack is answered and nothing else (a mote is
    // refused; a dye plant is answered as made).
    if let Some((status, flags)) = s
        .catalog
        .get(item)
        .and_then(|t| s.held_answers.get(&t.description.package))
        .copied()
    {
        let me = p_id(s, world);
        let m = use_answer(&u, me, Some(flags), false, 0.0, status);
        send(net, world, BlobKind::PRIVATE, &m);
        return;
    }
    // Coins go into the money bag.
    if crate::pack::coins_used(s, net, world, &u) {
        return;
    }
    // A surveyor's map leads to its hidden place, and conjures a resource node there.
    if crate::usable::maps::used(s, net, world, &u, clock) {
        return;
    }
    // A delving stone conjures a resource node.
    if crate::mining::survey(s, net, world, &u, clock) {
        return;
    }
    // A saddle or a mount's egg puts its user on its mount.
    if crate::mounts::used(s, net, world, &u, clock) {
        return;
    }
    let me = p_id(s, world);
    // A book is opened, then the use answered.
    if let Some(table) = crate::books::held(s, world, u.item) {
        send(
            net,
            world,
            BlobKind::PRIVATE,
            &crate::books::open(table, true, 0),
        );
        let done = use_done(&u, me, USE_FLAGS_PERSON, false, 0.0);
        send(net, world, BlobKind::PRIVATE, &done);
        return;
    }
    // A card whose use effect is a book: its picture shown, then the use answered.
    if let Some(b) = crate::books::shown_by(s, world, u.item) {
        send(
            net,
            world,
            BlobKind::PRIVATE,
            &crate::books::open(b.table, b.controls, b.picture),
        );
        let done = use_done(&u, me, crate::books::PICTURE_USE_FLAGS, false, 0.0);
        send(net, world, BlobKind::PRIVATE, &done);
        return;
    }
    // An item's own requirements (its place among them) come before the quest step its use moves:
    // away from its place it is refused, and told how far.
    if crate::quests::step_for(s, world, Trigger::Use(item), clock.unix).is_some() {
        let props = s
            .catalog
            .get(item)
            .map(|t| t.props.clone())
            .unwrap_or_default();
        if crate::item_use::refuse(s, net, world, &u, &props, clock) {
            return;
        }
    }
    if crate::quests::triggered(s, net, world, Trigger::Use(item), None, clock) {
        let done = use_done(&u, me, USE_FLAGS_PERSON, false, 0.0);
        send(net, world, BlobKind::PRIVATE, &done);
        return;
    }
    // An item whose use puts effects on its user: drunk, read, eaten...
    if crate::item_use::begin(s, net, world, &u, clock) {
        return;
    }
    crate::equip::use_held(s, net, world, &u, clock);
}

/// A held item used on an object, the player in reach: the quest step it triggers. One that takes
/// the player somewhere: the player plays the use, and two and a half seconds later the object
/// flashes and the player goes through portal space, the step's update made on arrival.
fn held_on_object<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    if crate::usable::doors::door(s, u.target).is_some() {
        crate::usable::doors::key_at_door(s, net, world, u, clock);
        return;
    }
    if crate::usable::chests::chest(s, u.target).is_some() {
        crate::usable::chests::key_at_chest(s, net, world, u, clock);
        return;
    }
    if crate::usable::vaults::vault(s, u.target).is_some() {
        crate::usable::vaults::glyph_at_vault(s, net, world, u, clock);
        return;
    }
    let item = s.things.get(u.item).map_or(0, |t| t.template);
    let object = presence::content_by_id(&s.content.objects, u.target);
    let (target, distance) = object.map_or((0, 0.0), |i| {
        let o = &s.content.objects[i];
        let d = s
            .players
            .get(&world)
            .and_then(|p| reach(&p.position, o.cell, o.origin))
            .unwrap_or(0.0);
        (o.template, d)
    });
    // An item of a set the object wants another of is refused, kept, and moves nothing.
    if let Some(i) =
        object.filter(|i| crate::wanting::judge(s, *i, item) == crate::wanting::Judged::Refused)
    {
        crate::wanting::refused(s, net, world, i);
        let m = use_answer_on(
            &u,
            crate::usable::interactions::FLAGS_KEPT,
            distance,
            USE_STATUS_OK,
        );
        send(net, world, BlobKind::PRIVATE, &m);
        return;
    }
    let trigger = Trigger::UseOn(item, target);
    // Every use is answered, so the item is free again: one that moves no step of the player's
    // quests is answered as done, the item kept.
    let Some(step) = crate::quests::step_for(s, world, trigger, clock.unix) else {
        s.unhandled.note(format!(
            "an item of template {item:08X} used on one of {target:08X}"
        ));
        let m = use_answer_on(
            &u,
            crate::usable::interactions::FLAGS_KEPT,
            distance,
            USE_STATUS_OK,
        );
        send(net, world, BlobKind::PRIVATE, &m);
        return;
    };
    // The step runs (on arrival, for one that leads somewhere): what the object wanted is met.
    if let Some(i) = object {
        crate::wanting::met(s, i, item);
    }
    let Some(place) = step.to else {
        let flags = if step.takes.contains(&item) {
            crate::usable::interactions::FLAGS_USED_UP
        } else {
            crate::usable::interactions::FLAGS_KEPT
        };
        crate::quests::triggered(s, net, world, trigger, None, clock);
        let m = use_answer_on(&u, flags, distance, USE_STATUS_OK);
        send(net, world, BlobKind::PRIVATE, &m);
        return;
    };
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.walking_to = None;
    let plays = ServerMessage::PlayBehavior(player_event(
        p,
        BehaviorParams {
            bits: BehaviorParams::BEHAVIOR | USE_BEHAVIOR_FLAGS | BehaviorParams::CONTEXT,
            behavior: behavior::USES_ON,
            context,
            // The words of flags 0x20 and 0x100.
            words: [0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
            ..BehaviorParams::default()
        },
    ));
    let id = p.character;
    let to = StoredPosition {
        cell: place.cell,
        origin: place.origin,
        rotation: crate::world::rotation_of_heading(place.heading),
    };
    p.trip = Some(Trip {
        to,
        used: Some(u),
        step: Some(step),
        resurrect: false,
        sent: false,
        said: None,
        updates: Vec::new(),
    });
    to_all_knowing(s, net, id, &plays);
    s.after(
        clock,
        OPENS_AFTER,
        Timer::Trip {
            world,
            step: Step::Opened,
        },
    );
}

/// Walk a player toward an object it asked to use; the use is carried out when it is in reach.
pub(crate) fn walk_to<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    range: f32,
) {
    walk_to_with(s, net, world, u, range, 0x63);
}

/// The movement order's flags for a walk to pick an item up.
const PICK_UP_WALK: u32 = 0x61;

/// Walk a player toward an object, the order's flags given.
fn walk_to_with<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    range: f32,
    flags: u32,
) {
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.move_to_stamp = p.move_to_stamp.wrapping_add(1);
    let m = move_order(
        p,
        MovementParams {
            flags,
            desired_velocity: 1.0,
            fail_distance: f32::MAX,
            desired_distance: range,
            max_success_distance: range,
            min_success_distance: 0.0,
            target: MoveTarget::Object(walk_goal(&u)),
            offset: Vec3::ZERO,
            offset_id: 0,
            waypoints: Vec::new(),
            context,
        },
    );
    p.walking_to = Some((u, range));
    p.walk_rest = None;
    let id = p.character;
    to_all_knowing(s, net, id, &m);
}

/// The body of a portal a player walks into: a sphere's radius and its center's height above the
/// portal's place.
pub const PORTAL_SPHERE: (f32, f32) = (0.8777, 0.8408);

/// A player's body as a portal touches it: two spheres' radius and their centers' heights above
/// its feet.
pub const PLAYER_SPHERES: (f32, [f32; 2]) = (0.555, [0.55, 1.50]);

/// The kinds of objects a player uses by walking into them (by script package and the packages
/// derived from them): the world's portals and vaults.
pub const USED_BY_WALKING_INTO: [&str; 2] = ["GenericPortal", "GenericVault"];

/// Does a player at a place touch an object's body there?
#[must_use]
pub fn touches(player: &StoredPosition, cell: u32, origin: [f32; 3]) -> bool {
    let (portal_r, portal_up) = PORTAL_SPHERE;
    let (player_r, heights) = PLAYER_SPHERES;
    let center = StoredPosition {
        cell,
        origin: [origin[0], origin[1], origin[2] + portal_up],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    heights.iter().any(|h| {
        let body = StoredPosition {
            origin: [player.origin[0], player.origin[1], player.origin[2] + h],
            ..*player
        };
        presence::distance(&body, &center).is_some_and(|d| d <= portal_r + player_r)
    })
}

/// A use of an object no request asked for (the player walked into it), as a client would have
/// described it: the object, the player, nothing it is used on.
#[must_use]
pub fn walked_in_use(item: u64, player: u64, from: StoredPosition) -> Use {
    use crate::graph::{tag, NULL};
    let none = (tag::REF, NULL);
    let int = |v: u32| (tag::INT, v);
    #[allow(clippy::cast_possible_truncation)]
    let long = |v: u64| [(tag::LONG, v as u32), (tag::HIGH, (v >> 32) as u32)];
    let mut slots = vec![none, none];
    slots.extend(long(item));
    slots.extend([none, int(0)]);
    slots.extend(long(player));
    slots.push(int((-1.0f32).to_bits()));
    slots.extend(long(0));
    slots.push(int(0));
    slots.extend(long(0));
    slots.extend([
        int(0),
        none,
        int(0),
        int(USE_FLAGS_TELEPORTED),
        int(0),
        int(0),
    ]);
    Use {
        item,
        target: 0,
        slots,
        message: None,
        from,
    }
}

/// A player has reported its place: a portal of a kind used by walking into it that its body has
/// come to touch takes it, as a use does; one it stays touching does not again.
pub fn walked<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let at = p.position;
    let me = p.character;
    let touched = s
        .content
        .objects
        .iter()
        .enumerate()
        .filter(|(i, o)| {
            (o.destination.is_some() || keeps_players_out(s, o))
                && s.walk_in_packages.contains(&o.description.package)
                && !crate::events::hides(s, *i)
        })
        .find(|(_, o)| touches(&at, o.cell, o.origin))
        .and_then(|(_, o)| s.placed.database_id(o));
    let before = s.touching.get(&world).copied();
    match touched {
        None => {
            s.touching.remove(&world);
        }
        Some(id) if before == Some(id) => {}
        Some(id) => {
            s.touching.insert(world, id);
            if crate::usable::vaults::vault(s, id).is_some() {
                let u = walked_in_use(id, me, at);
                crate::usable::vaults::needs_glyph(s, net, world, &u, id);
            } else if !actions_locked(s, world) {
                carry_out(s, net, world, walked_in_use(id, me, at), false, clock);
            }
        }
    }
}

/// A player walking to an object has reported its place: carry out the use once it is in reach.
pub fn arrived<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let Some((u, range)) = p.walking_to.clone() else {
        return;
    };
    let goal = walk_goal(&u);
    // What it was walking to has gone (taken, or its time up): there is nothing to arrive at, and it
    // isn't left walking, which would refuse all it does next.
    let there = s.things.get(goal).is_some_and(|t| t.at.is_some())
        || presence::content_by_id(&s.content.objects, goal).is_some();
    if !there {
        walk_cut_short(s, net, world);
        return;
    }
    // The client ends a walk by its own measure (the bodies, not their centers, within the range):
    // a player reporting itself at rest has arrived a little farther out.
    let still = p.heading_motion & 0x00FF_FFFF == crate::world::IDLE_MOTION & 0x00FF_FFFF;
    let slack = range + if still { STILL_SLACK } else { WALK_SLACK };
    if let Some(at) = s.things.get(goal).and_then(|t| t.at) {
        if presence::distance(&p.position, &at).is_some_and(|d| d <= slack) {
            let context = s.next_context;
            stop_walking(s, net, world);
            let lying = s
                .things
                .get(goal)
                .is_some_and(|t| t.kind == crate::things::Kind::Item && t.container.is_none());
            if crate::butchery::is_butchery(s, &u) {
                crate::butchery::begin(s, net, world, u, clock);
            } else if lying {
                crate::inventory::pick_up(s, net, world, goal, context, clock);
            } else {
                use_thing(s, net, world, u, clock);
            }
            return;
        }
        // Walked to an item to pick up, the player stands still where it last stood still (its
        // heading aside), out of reach: it could not get there, and the walk ends.
        let picking = s
            .things
            .get(goal)
            .is_some_and(|t| t.kind == crate::things::Kind::Item && t.container.is_none());
        let Some(p) = s.players.get_mut(&world) else {
            return;
        };
        let stuck = still
            && p.walk_rest
                .is_some_and(|r| r.cell == p.position.cell && r.origin == p.position.origin);
        p.walk_rest = still.then_some(p.position);
        if picking && stuck {
            crate::inventory::pickup_refused(
                s,
                net,
                world,
                goal,
                crate::inventory::refused::CANT_GET_THERE,
            );
            stop_walking(s, net, world);
        }
        return;
    }
    let Some(i) = presence::content_by_id(&s.content.objects, goal) else {
        return;
    };
    let o = &s.content.objects[i];
    // A placed object's body counts: the client ends a walk to it at the range from its side.
    let gap = walk_gap(
        s,
        &p.position,
        o.template,
        o.description.scale,
        o.cell,
        o.origin,
    );
    if gap.is_some_and(|d| d <= slack) {
        if u.target == 0 {
            carry_out(s, net, world, u, true, clock);
        } else {
            stop_walking(s, net, world);
            held_on_object(s, net, world, u, clock);
        }
    }
}

/// A use's answer when the walk to its object was cut short by the player moving by its own hand.
pub const MOVE_INTERRUPTED: u32 = 54;

/// The flag of a player's report that it moves by its own hand (its keys), not on a walk the
/// server ordered.
pub const MOVED_BY_HAND: u32 = 0x4;

/// The control flags of a use refused on its walk: a portal's and a lifestone's 820, a knife's on
/// a corpse 316, anything else's (a corpse, a person, a stone) 828.
fn walk_refusal_flags<S: Store>(s: &Server<S>, u: &Use) -> u32 {
    if crate::butchery::is_butchery(s, u) {
        return crate::usable::interactions::FLAGS_KEPT;
    }
    let place = presence::content_by_id(&s.content.objects, walk_goal(u)).is_some_and(|i| {
        let o = &s.content.objects[i];
        is_lifestone(s, o.description.package) || o.destination.is_some() || keeps_players_out(s, o)
    });
    if place {
        USE_FLAGS_BOUND
    } else {
        USE_FLAGS_TAKE_REFUSED
    }
}

/// A player walking to a use has reported moving by its own hand, the walk taken up (its report
/// carries the walk's stamp) and the object not yet in reach: the walk is stopped and the use
/// refused as interrupted, at the distance the player now stands from the object. A walk to pick
/// an item up is left to its own rule.
pub fn walk_interrupted<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    flags: u32,
    move_to_stamp: u16,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let Some((u, _)) = p.walking_to.clone() else {
        return;
    };
    if flags & MOVED_BY_HAND == 0 || move_to_stamp != p.move_to_stamp {
        return;
    }
    let goal = walk_goal(&u);
    if let Some(crate::things::Kind::Item) = s
        .things
        .get(goal)
        .filter(|t| t.container.is_none())
        .map(|t| t.kind)
    {
        return;
    }
    walk_cut_short(s, net, world);
}

/// A player on its way to an object goes no further: the walk ends, and its use is answered as cut
/// short. A trip or a death takes a player off its walk as its own moving does; the player would
/// otherwise stay walking to where it no longer is, and every later use would be refused.
pub(crate) fn walk_cut_short<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let Some((u, _)) = p.walking_to.clone() else {
        return;
    };
    let goal = walk_goal(&u);
    let here = p.position;
    let lying = s
        .things
        .get(goal)
        .is_some_and(|t| t.kind == crate::things::Kind::Item && t.container.is_none());
    if lying {
        let cant = crate::inventory::refused::CANT_GET_THERE;
        crate::inventory::pickup_refused(s, net, world, goal, cant);
        stop_walking(s, net, world);
        return;
    }
    let distance = match s.things.get(goal).map(|t| t.at) {
        Some(at) => at.and_then(|at| presence::distance(&here, &at)),
        None => presence::content_by_id(&s.content.objects, goal).and_then(|i| {
            let o = &s.content.objects[i];
            reach(&here, o.cell, o.origin)
        }),
    }
    .unwrap_or(0.0);
    let flags = walk_refusal_flags(s, &u);
    stop_walking(s, net, world);
    let no = use_refused(&u, 0, Some(flags), distance, MOVE_INTERRUPTED);
    send(net, world, BlobKind::PRIVATE, &no);
}

/// Carry out a use of a portal or a lifestone, the player in reach.
fn carry_out<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    walked: bool,
    clock: Clock,
) {
    let Some(i) = presence::content_by_id(&s.content.objects, u.item) else {
        return;
    };
    let o = s.content.objects[i].clone();
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.walking_to = None;
    if walked {
        let stop = move_order(
            p,
            MovementParams {
                flags: 0x40,
                desired_velocity: 0.0,
                fail_distance: 0.0,
                desired_distance: 0.0,
                max_success_distance: 0.0,
                min_success_distance: 0.0,
                target: MoveTarget::Place(Placement::default()),
                offset: Vec3::ZERO,
                offset_id: 0,
                waypoints: Vec::new(),
                context: 0,
            },
        );
        let id = p.character;
        to_all_knowing(s, net, id, &stop);
    }
    // An object a camp's living creature guards refuses the use; one carried out may take a
    // guarded object from the world for a while.
    if crate::guards::guarded(s, i) {
        crate::guards::refused(s, net, world, &u, i);
        return;
    }
    // One a shifting row keeps from the world does nothing; the one it stands moves on once used
    // (a person once its step has run).
    if crate::shifting::refused(s, net, world, &u, i) {
        return;
    }
    let person = talks_to(s, &o);
    carried_out(s, net, world, u, i, o, clock);
    crate::guards::used(s, net, i, clock);
    if !person {
        crate::shifting::used(s, net, i, clock);
    }
    // A hazard harms whoever uses it, after all else the use does.
    crate::hazards::used(s, net, world, i, clock);
}

/// What a use of a placed object does, the player in reach and the use allowed.
fn carried_out<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    i: usize,
    o: yalain_content::ContentObject,
    clock: Clock,
) {
    if crate::usable::halls::stone(s, u.item).is_some() {
        crate::usable::halls::at_stone(s, net, world, u, &o, clock);
        return;
    }
    if crate::usable::levers::is_lever(s, u.item) {
        crate::usable::levers::at_lever(s, net, world, &u, clock);
        return;
    }
    if crate::usable::doors::door(s, u.item).is_some() {
        crate::usable::doors::at_door(s, net, world, u, clock);
        return;
    }
    if crate::usable::chests::chest(s, u.item).is_some() {
        crate::usable::chests::at_chest(s, net, world, u, clock);
        return;
    }
    if crate::mining::quest_mine(s, o.template).is_some() {
        crate::mining::at_quest_mine(s, net, world, u, o.template, clock);
        return;
    }
    if s.shrine_packages.contains(&o.description.package) {
        crate::usable::shrines::at_shrine(s, net, world, &u, o.description.package, clock);
        return;
    }
    if is_lifestone(s, o.description.package) {
        bind(s, net, world, &u, &o, clock);
        return;
    }
    if let Some(b) = crate::market::broker(s, o.template) {
        let distance = reach(&u.from, o.cell, o.origin).unwrap_or(0.0);
        let done = use_done(&u, p_id(s, world), USE_FLAGS_PERSON, false, distance);
        crate::market::use_broker(s, net, world, u.item, b, &done, clock);
        return;
    }
    if talks_to(s, &o) {
        let distance = reach(&u.from, o.cell, o.origin).unwrap_or(0.0);
        let done = use_done(&u, p_id(s, world), USE_FLAGS_PERSON, false, distance);
        send(net, world, BlobKind::PRIVATE, &done);
        // A person who shows a book: the book, after the use's answer.
        if let Some(table) = crate::books::npc_book(s, o.template) {
            send(
                net,
                world,
                BlobKind::PRIVATE,
                &crate::books::open(table, true, 0),
            );
            return;
        }
        if crate::store::talk(s, net, world, u.item, &o, clock) {
            crate::shifting::used(s, net, i, clock);
        }
        return;
    }
    // A portal walked into that its quest keeps the player out of refuses as a use does (2017: 19
    // of 19 walk-ins into the generic portals kept by their instance's quest, flags 820, the
    // status a use's).
    if let Some(why) = quest_keeps_out(s, world, &o, clock) {
        let distance = reach(&u.from, o.cell, o.origin).unwrap_or(0.0);
        let no = use_refused(&u, 0, Some(USE_FLAGS_QUEST_KEEPS_OUT), distance, why);
        send(net, world, BlobKind::PRIVATE, &no);
        return;
    }
    let Some(to) = o.destination else {
        // An object only a hazard makes usable: the use is answered, as a person's.
        if crate::hazards::harms_at_use(s, i) {
            let distance = reach(&u.from, o.cell, o.origin).unwrap_or(0.0);
            let done = use_done(&u, p_id(s, world), USE_FLAGS_PERSON, false, distance);
            send(net, world, BlobKind::PRIVATE, &done);
        }
        return;
    };
    let to = StoredPosition {
        cell: to.cell,
        origin: to.origin,
        rotation: to.rotation,
    };
    // A person who sends whoever uses it says its line first (2017: 14 of 16 trips from the one
    // person recorded so came with its line).
    if let Some(line) = crate::talk::at_use(s, world, o.template, Some(i)) {
        let speaker = crate::talk::content_speaker(s, i);
        crate::talk::say(s, net, world, &speaker, &line.words, line.how);
    }
    if let Some(&scene) = s.story.shards.get(&u.item) {
        crate::scenes::shard_used(s, net, world, u, to, scene, clock);
        return;
    }
    if let Some(&scene) = s.story.doors.get(&(o.cell, o.template)) {
        through_door(s, net, world, u, to, scene, clock);
        return;
    }
    through_portal(s, net, world, u, to, clock);
}

/// Is a player on its way somewhere already? What it is on its way for (a kill's step, a glyph's
/// use) comes as it lands, so nothing else sends it anywhere first.
fn on_a_trip<S: Store>(s: &Server<S>, world: ConnId) -> bool {
    s.players.get(&world).is_some_and(|p| p.trip.is_some())
}

/// A player in reach of a portal goes through it: the portal flashes, the player is told it is
/// teleporting and goes into portal space, and comes out at `to` a while later.
pub(crate) fn through_portal<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    to: StoredPosition,
    clock: Clock,
) {
    if on_a_trip(s, world) {
        return;
    }
    flash_and_go(s, net, world, u.item);
    if let Some(p) = s.players.get_mut(&world) {
        p.trip = Some(Trip {
            to,
            used: Some(u),
            step: None,
            resurrect: false,
            sent: false,
            said: None,
            updates: Vec::new(),
        });
    }
    crate::quests::teleported(s, net, world, clock);
    s.after(
        clock,
        IN_PORTAL_SPACE,
        Timer::Trip {
            world,
            step: Step::Arrive,
        },
    );
}

/// What was used has sent the player: a follow ends (its order stopped, the player told), it
/// flashes, the player is told it is teleporting and goes into portal space (2017: a follow's stop
/// and "Stopped following." sent before "Teleporting ..." in 49 of 49 trips that told both).
fn flash_and_go<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, item: u64) {
    crate::follow::stop(s, net, world);
    if s.things.get(item).is_some() {
        let flash = ServerMessage::DoFx(ObjectEvent {
            sender: crate::inventory::visual(s, item),
            action: Fx {
                fx: fx::PORTAL_USED,
                scalar: 1.0,
            },
        });
        crate::combat::to_all_seeing(s, net, item, &flash);
    } else {
        let flash = ServerMessage::DoFx(ObjectEvent {
            sender: object_stamp(s, item),
            action: Fx {
                fx: fx::PORTAL_USED,
                scalar: 1.0,
            },
        });
        to_knowers(s, net, item, &flash);
    }
    let teleporting = TextInfo {
        string_id: text::TELEPORTING.0,
        table: text::TELEPORTING.1,
        ..TextInfo::default()
    };
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &crate::chat::show_text(8, teleporting),
    );
    enter_portal_space(s, net, world, true);
}

/// A step leads somewhere: the player is taken at once. A use of a person or an object has already
/// told its update (2017: the one such trip recorded, the touch's text and portal space together,
/// 0.18 s after the use, 1 of 1); a kill's step, given here, is run on landing, if it still may.
pub(crate) fn sends_on<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    by: u64,
    place: yalain_content::quests::Place,
    step: Option<yalain_content::quests::Step>,
    clock: Clock,
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if p.trip.is_some() {
        return;
    }
    p.trip = Some(Trip {
        to: StoredPosition {
            cell: place.cell,
            origin: place.origin,
            rotation: crate::world::rotation_of_heading(place.heading),
        },
        used: None,
        step,
        resurrect: false,
        sent: false,
        said: None,
        updates: Vec::new(),
    });
    s.after(
        clock,
        0.0,
        Timer::Trip {
            world,
            step: Step::Sent { by },
        },
    );
}

/// A door used: its scene in place of portal space, in one tick: told it is teleporting, its
/// animation and movement frozen, the scene begun; no flash and no glows. A second and a half later
/// it comes out at the door's destination as from portal space, the scene ended in place of portal
/// space's exit.
fn through_door<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    to: StoredPosition,
    scene: u32,
    clock: Clock,
) {
    if on_a_trip(s, world) {
        return;
    }
    crate::follow::stop(s, net, world);
    let teleporting = TextInfo {
        string_id: text::TELEPORTING.0,
        table: text::TELEPORTING.1,
        ..TextInfo::default()
    };
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &crate::chat::show_text(8, teleporting),
    );
    crate::effects::teleported(s, net, world);
    send(net, world, BlobKind::PRIVATE, &animation_frozen(true));
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &session::movement_frozen(true),
    );
    let enter = portal_scene(s, function::ENTER_PORTAL_SCENE, scene);
    send(net, world, BlobKind::PRIVATE, &enter);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.scene_drawn = false;
    p.trip = Some(Trip {
        to,
        used: Some(u),
        step: None,
        resurrect: false,
        sent: false,
        said: None,
        updates: Vec::new(),
    });
    crate::quests::teleported(s, net, world, clock);
    s.after(
        clock,
        IN_PORTAL_SPACE,
        Timer::Trip {
            world,
            step: Step::OutOfScene { scene },
        },
    );
}

/// A shard's trip into portal space: told it is teleporting, frozen, into portal space, then the
/// shard's glows and the teleport's effects. The player's auras stay on (2017: the two shard trips
/// taken with auras on, seven of them, turned none off).
pub(crate) fn shard_portal_space<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    crate::follow::stop(s, net, world);
    let teleporting = TextInfo {
        string_id: text::TELEPORTING.0,
        table: text::TELEPORTING.1,
        ..TextInfo::default()
    };
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &crate::chat::show_text(8, teleporting),
    );
    crate::effects::teleported(s, net, world);
    send(net, world, BlobKind::PRIVATE, &animation_frozen(true));
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &session::movement_frozen(true),
    );
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &portal_space(function::ENTER_PORTAL_SPACE),
    );
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.scene_drawn = false;
    let id = p.character;
    let fxs: Vec<ServerMessage> = [
        crate::scenes::look::GLOW,
        crate::scenes::look::SHIMMER,
        fx::TELEPORT,
        ARRIVAL_FX,
    ]
    .into_iter()
    .map(|f| ServerMessage::DoFx(player_event(p, Fx { fx: f, scalar: 1.0 })))
    .collect();
    for m in &fxs {
        to_all_knowing(s, net, id, m);
    }
}

/// The player goes into portal space: its effects, its animation and movement frozen, its toggled
/// skills off.
fn enter_portal_space<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    teleport_fx: bool,
) {
    goes_into_portal_space(s, net, world, teleport_fx);
    // Toggled skills (auras among them) go off as the player goes into portal space.
    crate::effects::toggles_off_all(s, net, world);
}

/// The player goes into portal space: its effects, its animation and movement frozen, and any walk
/// it was on over.
fn goes_into_portal_space<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    teleport_fx: bool,
) {
    walk_cut_short(s, net, world);
    crate::effects::teleported(s, net, world);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let id = p.character;
    let teleport = teleport_fx.then(|| {
        ServerMessage::DoFx(player_event(
            p,
            Fx {
                fx: fx::TELEPORT,
                scalar: 1.0,
            },
        ))
    });
    let arrival = ServerMessage::DoFx(player_event(
        p,
        Fx {
            fx: ARRIVAL_FX,
            scalar: 1.0,
        },
    ));
    p.scene_drawn = false;
    if let Some(t) = teleport {
        to_all_knowing(s, net, id, &t);
    }
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &flag(function::ANIMATION_FROZEN, true),
    );
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &session::movement_frozen(true),
    );
    to_all_knowing(s, net, id, &arrival);
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &portal_space(function::ENTER_PORTAL_SPACE),
    );
}

/// Bind a player to the lifestone it used.
fn bind<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    o: &yalain_content::ContentObject,
    clock: Clock,
) {
    let stone = ServerMessage::PlayBehavior(ObjectEvent {
        sender: object_stamp(s, u.item),
        action: BehaviorParams::behavior(behavior::LIFESTONE_BINDS),
    });
    to_knowers(s, net, u.item, &stone);
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let mut b = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR | 0x20 | 0x100 | BehaviorParams::CONTEXT,
        behavior: behavior::PLAYER_BINDS,
        context,
        ..BehaviorParams::default()
    };
    b.words[1] = 1;
    b.words[6] = 1;
    let binds = ServerMessage::PlayBehavior(player_event(p, b));
    // A recall lands where the player stood as it bound, facing as it faced.
    let landing = p.position;
    p.places.retain(|(k, _)| *k != kind::LIFESTONE);
    p.places.push((kind::LIFESTONE, landing));
    let mut bound = u.clone();
    bound.message = Some(TextInfo {
        string_id: text::BOUND.0,
        table: text::BOUND.1,
        ..TextInfo::default()
    });
    let distance = reach(&u.from, o.cell, o.origin).unwrap_or(0.0);
    let done = use_done(&bound, p.character, USE_FLAGS_BOUND, false, distance);
    let id = p.character;
    to_all_knowing(s, net, id, &binds);
    send(net, world, BlobKind::PRIVATE, &done);
    crate::world::keep_player(s, world);
    s.after(clock, BIND_PLAYS, Timer::BindEnds { world });
}

/// How long a player's binding behavior plays before it is stopped, in seconds (2017: a median
/// 0.99 s after it began, 280 of 284 later than its own tick).
pub const BIND_PLAYS: f64 = 0.99;

/// A player's binding behavior has played: it is stopped, for all who see the player.
pub fn bind_ends<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let stop = ServerMessage::StopBehavior(player_event(p, Word(behavior::PLAYER_BINDS)));
    let id = p.character;
    to_all_knowing(s, net, id, &stop);
}

/// The place beside a target (a lifestone, a character) where a mover lands: `BESIDE_LIFESTONE`
/// meters from it toward where the mover stood when in its landblock (else south of it), facing
/// it, at the target's height. Its cell is the one it lies in, never the mover's: outdoors the
/// land cell under it, in the landblock it falls in; inside the target's own (2017: each of 3,130
/// arrivals on the land was placed in the land cell under it, and none of the 9 returns from
/// another land cell of the same landblock kept the cell it left).
#[must_use]
pub fn beside(cell: u32, target: [f32; 3], from: &StoredPosition) -> StoredPosition {
    let (dx, dy) = if from.cell >> 16 == cell >> 16 {
        (from.origin[0] - target[0], from.origin[1] - target[1])
    } else {
        (0.0, -1.0)
    };
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if len > 0.01 {
        (dx / len, dy / len)
    } else {
        (0.0, -1.0)
    };
    let (x, y) = (
        target[0] + ux * BESIDE_LIFESTONE,
        target[1] + uy * BESIDE_LIFESTONE,
    );
    let (cell, x, y) = if presence::indoors(cell) {
        (cell, x, y)
    } else {
        crate::spawn::outdoor(cell >> 16, x, y)
    };
    // Facing the target: degrees clockwise from north (+y), from the landing back to it.
    let deg = math::atan2f(-ux, -uy).to_degrees().rem_euclid(360.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = u8::try_from((deg * 255.0 / 360.0) as u32 % 256).unwrap_or(0);
    StoredPosition {
        cell,
        origin: [x, y, target[2]],
        rotation: crate::world::rotation_of_heading(byte),
    }
}

/// The place map coordinates (east, north) name, as the client turns them into a place: the
/// middle of the land cell there (each 20 m across, eight to a landblock), standing on the ground
/// where the server holds the ground; none off the land.
#[must_use]
pub fn of_map_coords<S: Store>(s: &Server<S>, east: f32, north: f32) -> Option<StoredPosition> {
    let mut p = dereth_primitives::Position::new(
        dereth_primitives::CellId(0),
        dereth_primitives::Frame::new(
            dereth_primitives::Vec3::ZERO,
            dereth_primitives::Quat::IDENTITY,
        ),
    );
    if !auberean_physics::geom::position::set_from_map_coords(&mut p, east, north) {
        return None;
    }
    let mut at = StoredPosition {
        cell: p.cell.0,
        origin: [p.frame.origin.x, p.frame.origin.y, 0.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    if let Some(z) = s.ground.as_ref().and_then(|g| g.height(at.cell, at.origin)) {
        at.origin[2] = z;
    }
    Some(at)
}

/// [`beside`], on the ground there: outdoors at the land's height where it lands (when the ground
/// is known), else at the target's own height.
#[must_use]
pub fn beside_on_ground<S: Store>(
    s: &Server<S>,
    cell: u32,
    target: [f32; 3],
    from: &StoredPosition,
) -> StoredPosition {
    let mut at = beside(cell, target, from);
    if let Some(z) = (!presence::indoors(at.cell))
        .then(|| s.ground.as_ref()?.height(at.cell, at.origin))
        .flatten()
    {
        at.origin[2] = z;
    }
    at
}

/// A recall takes the player into portal space toward the place it lands (its powering up and
/// its story told by the skill's use), and what it says as it arrives.
pub fn recall_trip<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    to: StoredPosition,
    said: Option<TextInfo>,
    clock: Clock,
) {
    if on_a_trip(s, world) {
        return;
    }
    enter_portal_space(s, net, world, false);
    if let Some(p) = s.players.get_mut(&world) {
        p.trip = Some(Trip {
            to,
            used: None,
            step: None,
            resurrect: false,
            sent: false,
            said,
            updates: Vec::new(),
        });
    }
    crate::quests::teleported(s, net, world, clock);
}

/// A recall comes out of portal space where it lands.
pub fn recall_arrives<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    clock: Clock,
) {
    arrive(s, net, world, None, clock);
}

/// An effect sends a player to a place: into portal space as a recall goes, with no word of it
/// before or after; a second and a half later, the arrival. A player already on its way somewhere
/// is left to it.
pub fn sent_away<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    to: StoredPosition,
    clock: Clock,
) {
    if s.players.get(&world).is_none_or(|p| p.trip.is_some()) {
        return;
    }
    enter_portal_space(s, net, world, false);
    if let Some(p) = s.players.get_mut(&world) {
        p.trip = Some(Trip {
            to,
            used: None,
            step: None,
            resurrect: false,
            sent: true,
            said: None,
            updates: Vec::new(),
        });
    }
    crate::quests::teleported(s, net, world, clock);
    s.after(
        clock,
        IN_PORTAL_SPACE,
        Timer::Trip {
            world,
            step: Step::Arrive,
        },
    );
}

/// An item's use sends its user to a place: into portal space now, out of it at the place a moment
/// later, saying what the item says there; the item is spent and its use answered as it comes out
/// ([`crate::item_use::arrived`]). A player already on its way somewhere is left to it. Returns
/// whether it went.
pub fn item_trip<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    (to, used, said): (StoredPosition, Use, Option<TextInfo>),
    clock: Clock,
) -> bool {
    if on_a_trip(s, world) {
        return false;
    }
    enter_portal_space(s, net, world, false);
    if let Some(p) = s.players.get_mut(&world) {
        p.trip = Some(Trip {
            to,
            used: Some(used),
            step: None,
            resurrect: false,
            sent: false,
            said,
            updates: Vec::new(),
        });
    }
    crate::quests::teleported(s, net, world, clock);
    s.after(
        clock,
        IN_PORTAL_SPACE,
        Timer::Trip {
            world,
            step: Step::Arrive,
        },
    );
    true
}

/// Does using this portal leave its user a place to recall to (its portal flags)? A portal of the
/// world whose flags say so, or one an item's use made whose template's do, used itself (not an
/// item used on it); a summoned portal never does.
fn recalled_to<S: Store>(s: &Server<S>, u: &Use) -> bool {
    let flags = presence::content_by_id(&s.content.objects, u.item)
        .map(|i| s.content.objects[i].facts.portal)
        .or_else(|| crate::usable::portals::portal(s, u.item).map(|t| t.facts.portal));
    u.target == 0 && flags.is_some_and(|f| f & yalain_content::portal::RECALL != 0)
}

/// Where a dead player returns: the lifestone it is bound to; never bound, beside the nearest
/// lifestone the world has in its landblock or one beside it (authored: 2017 shows no unbound
/// death), on the ground there, not at the height it fell from; else where it fell. Outdoors the
/// ground is the land's height where it lands, inside the lifestone's own (2017: 136 of 137 returns
/// were placed at the height their client then stood at).
#[must_use]
pub fn resurrection_place<S: Store>(s: &Server<S>, world: ConnId) -> StoredPosition {
    let Some(p) = s.players.get(&world) else {
        return StoredPosition {
            cell: 0,
            origin: [0.0; 3],
            rotation: [1.0, 0.0, 0.0, 0.0],
        };
    };
    resurrection_place_of(s, &p.places, p.position)
}

/// [`resurrection_place`] for a character with these kept places, fallen at `fell`.
#[must_use]
pub fn resurrection_place_of<S: Store>(
    s: &Server<S>,
    places: &[(u32, StoredPosition)],
    fell: StoredPosition,
) -> StoredPosition {
    if let Some((_, at)) = places.iter().find(|(k, _)| *k == kind::LIFESTONE) {
        return *at;
    }
    s.content
        .objects
        .iter()
        .filter(|o| is_lifestone(s, o.description.package))
        .filter_map(|o| reach(&fell, o.cell, o.origin).map(|d| (d, o)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(fell, |(_, o)| beside_on_ground(s, o.cell, o.origin, &fell))
}

/// A dead player goes through portal space to where it returns (its effects, frozen, portal
/// space; a second and a half later, the arrival).
pub fn resurrection_trip<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    to: StoredPosition,
    clock: Clock,
) {
    // Its auras stay on (2017: 0 of 98 went off at the 44 requests of a player holding some).
    goes_into_portal_space(s, net, world, false);
    if let Some(p) = s.players.get_mut(&world) {
        p.trip = Some(Trip {
            to,
            used: None,
            step: None,
            resurrect: true,
            sent: false,
            said: None,
            updates: Vec::new(),
        });
    }
    s.after(
        clock,
        IN_PORTAL_SPACE,
        Timer::Trip {
            world,
            step: Step::Arrive,
        },
    );
}

/// Run a step of a player's trip.
pub fn step<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    step: Step,
    clock: Clock,
) {
    match step {
        Step::Opened => {
            if !crate::usable::vaults::opens(s, net, world, clock) {
                return;
            }
            let Some(target) = s
                .players
                .get(&world)
                .and_then(|p| p.trip.as_ref())
                .and_then(|t| t.used.as_ref())
                .map(|u| u.target)
            else {
                return;
            };
            // A follow ends before the flash, as at a portal.
            crate::follow::stop(s, net, world);
            let flash = ServerMessage::DoFx(ObjectEvent {
                sender: object_stamp(s, target),
                action: Fx {
                    fx: fx::PORTAL_USED,
                    scalar: 1.0,
                },
            });
            to_knowers(s, net, target, &flash);
            let teleporting = TextInfo {
                string_id: text::TELEPORTING.0,
                table: text::TELEPORTING.1,
                ..TextInfo::default()
            };
            send(
                net,
                world,
                BlobKind::PRIVATE,
                &crate::chat::show_text(8, teleporting),
            );
            enter_portal_space(s, net, world, true);
            crate::quests::teleported(s, net, world, clock);
            s.after(
                clock,
                IN_PORTAL_SPACE,
                Timer::Trip {
                    world,
                    step: Step::Arrive,
                },
            );
            // An item's use whose step takes the fellowship along sends the fellows as the user goes.
            let carried = s
                .players
                .get(&world)
                .and_then(|p| p.trip.as_ref())
                .and_then(|t| t.step.as_ref())
                .filter(|st| st.to_fellows)
                .and_then(|st| st.to);
            if let Some(place) = carried {
                crate::quests::fellows_go(s, world, target, place, clock);
            }
        }
        Step::Sent { by } => {
            if s.players.get(&world).is_none_or(|p| p.trip.is_none()) {
                return;
            }
            flash_and_go(s, net, world, by);
            crate::quests::teleported(s, net, world, clock);
            s.after(
                clock,
                IN_PORTAL_SPACE,
                Timer::Trip {
                    world,
                    step: Step::Arrive,
                },
            );
        }
        Step::Arrive => arrive(s, net, world, None, clock),
        Step::OutOfScene { scene } => arrive(s, net, world, Some(scene), clock),
        Step::Shard { scene } => crate::scenes::bestow(s, net, world, scene, clock),
    }
}

/// Out of portal space (or a door's scene), at the destination.
fn arrive<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    scene: Option<u32>,
    clock: Clock,
) {
    let time = net.game_time(clock.local);
    // A follow still on ends as the player comes out, before the exit: a trip told "Teleporting ..."
    // ended it as it went (2017: 5 of 5 trips within the world with no such text that came out on
    // the connection that went in, sent the order stopped, "Stopped following.", then the exit).
    if on_a_trip(s, world) {
        crate::follow::stop(s, net, world);
    }
    let Some(mut p) = s.players.remove(&world) else {
        return;
    };
    let Some(trip) = p.trip.take() else {
        s.players.insert(world, p);
        return;
    };
    let exit = match scene {
        Some(scene) => portal_scene(s, function::EXIT_PORTAL_SCENE, scene),
        None => portal_space(function::EXIT_PORTAL_SPACE),
    };
    send(net, world, BlobKind::PRIVATE, &exit);
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &flag(function::ANIMATION_FROZEN, false),
    );
    if !trip.resurrect && p.protected.is_some() {
        // Out of portal space somewhere else: a lifestone protection ends.
        s.players.insert(world, p);
        crate::effects::unprotect(s, net, world, |_, _| {});
        let Some(q) = s.players.remove(&world) else {
            return;
        };
        p = q;
    }
    if trip.resurrect {
        s.players.insert(world, p);
        crate::death::returns(s, net, world);
        let Some(q) = s.players.remove(&world) else {
            return;
        };
        p = q;
    }
    if let Some(step) = &trip.step {
        // The step the trip was for: its update now, if nothing has moved its quest since (the
        // player back in the list for it).
        s.players.insert(world, p);
        if crate::quests::may_run_now(s, world, step, clock.unix) {
            crate::quests::run(s, net, world, step, None, clock);
        }
        let Some(q) = s.players.remove(&world) else {
            return;
        };
        p = q;
    }
    if !trip.updates.is_empty() {
        // The updates whose recall this trip is, told as it comes out (2017: the Gauntlets' 47 of
        // 47 advances, in portal space's exit's tick, 1.44 to 1.67 s after the trip began).
        s.players.insert(world, p);
        for (quest, kind) in &trip.updates {
            crate::quests::told_on_arrival(s, net, world, *quest, *kind, clock);
        }
        let Some(q) = s.players.remove(&world) else {
            return;
        };
        p = q;
    }
    let shard_quest = trip
        .used
        .as_ref()
        .and_then(|u| s.story.shard_quests.get(&u.item).copied());
    if let Some(quest) = shard_quest {
        // A vault's shard completes its quest as the player comes out, before the use is answered
        // (2017: 33 of 33).
        s.players.insert(world, p);
        crate::quests::apply(
            s,
            net,
            world,
            quest,
            yalain_content::quests::Update::Complete,
            0,
            clock,
        );
        let Some(q) = s.players.remove(&world) else {
            return;
        };
        p = q;
    }
    // An item of the pack that sent the player: carried out once the player is out.
    let from_pack = trip
        .used
        .as_ref()
        .filter(|u| u.target == 0 && p.play.items.iter().any(|h| h.id == u.item));
    if let Some(u) = trip
        .used
        .as_ref()
        .filter(|u| u.target == 0 && from_pack.is_none())
    {
        // The portal's own effects, put on the player as it comes out, before the use is answered
        // (2017: 3 of 3 such portals' uses: quests given and moved on, and what they put on). A
        // portal standing in the world carries its template's as a placed one does.
        let template = presence::content_by_id(&s.content.objects, u.item)
            .map(|i| s.content.objects[i].template)
            .or_else(|| crate::usable::portals::portal(s, u.item).map(|t| t.template));
        let effects = template
            .and_then(|t| s.item_uses.get(&t).cloned())
            .unwrap_or_default();
        if !effects.is_empty() {
            let me = p.character;
            s.players.insert(world, p);
            for e in effects {
                crate::effects::apply(
                    s,
                    net,
                    world,
                    e.state,
                    crate::effects::Cast {
                        caster: me,
                        spellcraft: e.spellcraft,
                        ..crate::effects::Cast::default()
                    },
                    clock,
                );
            }
            let Some(q) = s.players.remove(&world) else {
                return;
            };
            p = q;
        }
        let distance = presence::content_by_id(&s.content.objects, u.item)
            .and_then(|i| {
                let o = &s.content.objects[i];
                reach(&u.from, o.cell, o.origin)
            })
            .or_else(|| {
                let at = s.things.get(u.item).and_then(|t| t.at)?;
                presence::distance(&u.from, &at)
            })
            .unwrap_or(0.0);
        send(
            net,
            world,
            BlobKind::PRIVATE,
            &use_done(u, p.character, USE_FLAGS_TELEPORTED, true, distance),
        );
    }
    // A recall says where it came out before the player's place (2017: every recall's text before
    // its position).
    if let Some(said) = trip.said.clone() {
        send(
            net,
            world,
            BlobKind::PRIVATE,
            &crate::chat::show_text(2, said),
        );
    }
    if let Some(u) = from_pack {
        s.players.insert(world, p);
        crate::item_use::arrived(s, net, world, u);
        let Some(q) = s.players.remove(&world) else {
            return;
        };
        p = q;
    }
    // Clients that held the player at the old place and do not see the new one see it leave.
    let to = trip.to;
    presence::leave_from(s, net, &p, |q| {
        !presence::in_view(&q.position, to.cell, to.origin)
    });
    p.teleport_stamp = p.teleport_stamp.wrapping_add(1);
    p.position = trip.to;
    p.travel = crate::reported::Travel::placed(clock.local.0);
    p.heading_motion =
        u32::from(heading_of_rotation(trip.to.rotation)) << 24 | crate::world::IDLE_MOTION;
    if trip.used.as_ref().is_some_and(|u| recalled_to(s, u)) {
        p.places.retain(|(k, _)| *k != kind::LAST_PORTAL);
        p.places.push((kind::LAST_PORTAL, trip.to));
    }
    let m = p.next_position(time, ON_GROUND);
    // The client lets go of what it knew at the old place by itself: it is told again of all
    // that is around the new one.
    p.known.clear();
    s.players.insert(world, p);
    presence::tell_position(s, net, world, &m, true, false);
    // Landed in another chat region (or indoors from one): its channels again.
    crate::chat::region_moved(s, net, world);
    // A trip through portal space ends an attack run: the player is no longer attacking, and at
    // peace (2017: 5 of 5 runs, just after the exit).
    if s.players.get(&world).is_some_and(|p| p.attacking.is_some()) {
        crate::combat::end(s, net, world);
    }
    if trip.resurrect {
        crate::death::returned(s, net, world);
        s.after(
            clock,
            crate::death::MOVES_AFTER_RETURN,
            crate::Timer::MovesAgain { world },
        );
        s.after(
            clock,
            crate::effects::AURAS_PULSE_AFTER_RETURN,
            crate::Timer::AurasPulseAgain { world },
        );
    }
    if let Some(u) = trip.used.as_ref().filter(|u| u.target != 0) {
        // An item used on an object: the use answered, as the object's work is done (a vault's
        // glyph used up first).
        let u = &crate::usable::vaults::arrived(s, net, world, u, clock);
        let distance = presence::content_by_id(&s.content.objects, u.target)
            .and_then(|i| {
                let o = &s.content.objects[i];
                reach(&u.from, o.cell, o.origin)
            })
            .unwrap_or(0.0);
        let me = p_id(s, world);
        send(
            net,
            world,
            BlobKind::PRIVATE,
            &use_done(u, me, USE_FLAGS_UNLOCKED, true, distance),
        );
    }
    // Arriving through a portal may move quests on: those of its template, then those of that one
    // placed portal.
    if let Some((template, object)) = trip
        .used
        .as_ref()
        .filter(|u| u.target == 0 && trip.step.is_none())
        .and_then(|u| presence::content_by_id(&s.content.objects, u.item).map(|i| (i, u.item)))
        .map(|(i, object)| (s.content.objects[i].template, object))
    {
        crate::quests::triggered(s, net, world, Trigger::Arrive(template), None, clock);
        let from = Trigger::ArriveFrom(template, object);
        crate::quests::triggered(s, net, world, from, None, clock);
    } else if let Some(template) = trip
        .used
        .as_ref()
        .filter(|u| u.target == 0 && trip.step.is_none())
        .and_then(|u| crate::usable::portals::portal(s, u.item))
        .map(|t| t.template)
    {
        // One standing in the world moves those of its template: it is no placed portal (2017: a
        // master vault's portal, seen made at three places, gave its quest on arrival, 2 sessions).
        crate::quests::triggered(s, net, world, Trigger::Arrive(template), None, clock);
    }
    // Its pets come out beside it, but for an escort its trip fails.
    crate::escorts::owner_arrives(s, net, world, clock);
    crate::pets::owner_arrives(s, net, world, time);
    presence::refresh(s, net, world);
    // A rider come out inside is soon off its mount.
    crate::mounts::arrived(s, world, clock);
    crate::world::keep_player(s, world);
}
