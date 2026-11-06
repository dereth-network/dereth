//! Chests: a player walks to a chest and opens it; what it holds is made anew at each opening from
//! its generator profile and shown to the opener alone; the player takes from it as from a corpse
//! and closes it.
//!
//! **Opening.** Within two meters, the chest flashes and plays its opening for everyone who sees
//! it, and what an earlier opening left in it is cleared. When its opening has played the opener
//! is told what it holds and everyone sees it open. A plain opening is not answered as a use.
//!
//! **Locks.** A lockable chest locked by its kind refuses a plain use: the opening is answered
//! locked and the chest flashes. The key whose id is the chest's is used on it from within three
//! meters: the player turns it, and a moment later a chest that opens on unlocking opens; as it is
//! open the key is used up and the use answered "Your key fits the lock.". Another kind of chest
//! only unlocks, the key used up and the use answered, and is then opened by a plain use. A chest
//! that locks on closing locks again as it closes.
//!
//! **Treasure.** What a chest draws, where its profile's row names a level, is treasure of that
//! level as a corpse's is, each piece worth a draw from the row's band (2017: 205 pieces in 8 kinds of
//! chest, each kind's values and least levels its own: least levels 8 to 16 worth 3 to 7 in the
//! middle half, 15 to 20 worth 20 to 30, 41 to 50 worth 50 to 72 or 130 to 189). A profile's extras
//! aren't.
//!
//! **Quests.** A chest a quest keeps refuses at once a player whose quest is not at the phase it
//! names, and again at the chest. What its kind names for its use comes on its opener as it begins
//! to open, so a kind that completes its own quest opens once for each giving.
//!
//! **Closing.** Asked to close, or its opener gone more than six meters from it, the chest plays its
//! closing for everyone; when that has played the closer is told and everyone sees it shut.

use std::collections::{HashMap, HashSet};

use auberean_protocol::physics::{Fx, ObjectEvent, Word};
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::script::{ClientCall, FunctionId, ObjectCall, ScriptArgs};
use auberean_protocol::types::BehaviorParams;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::inventory::{function, id_list, look, q};
use crate::places::{self, Use};
use crate::{presence, progress, session, Clock, Server, Timer};

/// The script package every chest derives from.
pub const PACKAGE: &str = "ChestTemplate";

/// How close a player walks to a chest to open it, and to use a key on it, in meters (2017:
/// `MoveTo` to 2.0 m, a key's to 3.0 m).
pub const REACH: f32 = 2.0;
pub const KEY_REACH: f32 = 3.0;

/// How long a key turns before the chest moves, in seconds (2017: 1.34 s median, 1.26-1.39).
pub const KEY_TURNING: f64 = 1.34;

/// Why a chest's opening is refused: it is locked.
pub const LOCKED: u32 = 10_030;

/// A chest a quest keeps refuses a player whose quest is not at its phase: the use's status and
/// control flags.
pub const WRONG_QUEST: u32 = 10_019;
pub const WRONG_QUEST_FLAGS: u32 = 828;

/// How far its opener may go from an open chest before the chest closes, in meters (2017's servers
/// closed chests and corpses on a tick a second apart; at this distance their closes were told
/// from their earlier ticks 94 times in 100).
pub const WALK_AWAY: f32 = 6.0;

/// The call that clears an item from a container the client knows.
pub const DESTROY_CONTAINED: u32 = 0x820E_005C;

/// How long a chest's opening and closing play, in seconds, by its template (2017: the sigil
/// caches and troves 3.95 and 3.94 s, Isin Dule's Hoard 3.25 and 1.71 s, the High Shaman's Cache
/// 2.99 and 0.94 s, the Ultra Chest 0.96 and 0.28 s, the medallion chest 0.93 and 0.32 s; the small
/// chests about 1.27 and 0.75 s).
#[must_use]
pub fn animation(template: u32) -> (f64, f64) {
    match template {
        0x4700_0F51 | 0x4700_0F58..=0x4700_0F5E | 0x4700_0F61..=0x4700_0F64 | 0x4700_0F6A => {
            (3.95, 3.94)
        }
        0x4700_0F6F => (3.25, 1.71),
        0x4700_0F4C => (2.99, 0.94),
        0x4700_0F39 => (0.96, 0.28),
        0x4700_1867 => (0.93, 0.32),
        _ => (1.27, 0.75),
    }
}

/// Where a chest is in its moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Closed,
    Opening,
    Open,
    Closing,
}

/// A use waiting on a chest: the player, its use, how far it stood, the key it used (its id).
#[derive(Debug, Clone, PartialEq)]
pub struct Waiting {
    pub world: ConnId,
    pub used: Use,
    pub distance: f32,
    pub key: Option<u64>,
}

/// One chest's state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chest {
    pub phase: Phase,
    pub generation: u32,
    /// What it holds (made at its last opening).
    pub contents: Vec<u64>,
    /// The player who has it open.
    pub opener: Option<ConnId>,
    /// Its key has unlocked it.
    pub unlocked: bool,
    pub waiting: Option<Waiting>,
}

/// The chests' states, and the script packages of chests.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chests {
    pub by_id: HashMap<u64, Chest>,
    pub packages: HashSet<u32>,
}

/// A step of a chest's moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// A key has turned.
    Turned,
    /// The opening has played.
    Opened,
    /// The closing has played.
    Closed,
}

/// What a chest's template says of its lock and contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Kind {
    pub template: u32,
    /// Locked until its key opens it.
    pub locked: bool,
    /// Its key opens it, rather than only unlocking it.
    pub opens_on_unlock: bool,
    pub locks_on_close: bool,
    pub key: Option<u32>,
    pub profile: Option<u32>,
    pub most: Option<usize>,
}

/// A chest of the world, by its id: its kind and where it stands.
#[must_use]
pub fn chest<S: Store>(s: &Server<S>, id: u64) -> Option<(Kind, StoredPosition)> {
    let i = presence::content_by_id(&s.content.objects, id)?;
    let o = &s.content.objects[i];
    if o.destination.is_some() || !s.chests.packages.contains(&o.description.package) {
        return None;
    }
    let props = s.catalog.get(o.template).map(|t| &t.props);
    let flag = |k: &str| props.is_some_and(|p| p.flag(k));
    let lockable = flag("Lock_Lockable");
    let kind = Kind {
        template: o.template,
        // A chest that opens on unlocking is locked although its kind does not say so (2017: every
        // opening of one was by its key, and its one plain use refused).
        locked: lockable && (flag("Lock_IsLocked") || flag("Lock_OpenOnUnlock")),
        opens_on_unlock: flag("Lock_OpenOnUnlock"),
        locks_on_close: flag("Lock_LockOnClose"),
        key: props.and_then(|p| p.word("Lock_KeyID")),
        profile: props.and_then(|p| p.word("GeneratorProfile")),
        most: props
            .and_then(|p| p.int("Container_MaxContainerSize"))
            .and_then(|m| usize::try_from(m).ok()),
    };
    Some((
        kind,
        StoredPosition {
            cell: o.cell,
            origin: o.origin,
            rotation: o.rotation,
        },
    ))
}

fn locked<S: Store>(s: &Server<S>, id: u64, kind: &Kind) -> bool {
    kind.locked && !s.chests.by_id.get(&id).is_some_and(|c| c.unlocked)
}

fn position<S: Store>(s: &Server<S>, world: ConnId) -> Option<StoredPosition> {
    s.players.get(&world).map(|p| p.position)
}

/// What an open chest still holds, in its order.
#[must_use]
pub fn contents<S: Store>(s: &Server<S>, id: u64) -> Option<Vec<u64>> {
    let c = s.chests.by_id.get(&id)?;
    Some(
        c.contents
            .iter()
            .copied()
            .filter(|i| s.things.get(*i).and_then(|t| t.container).map(|c| c.0) == Some(id))
            .collect(),
    )
}

/// Is a player's use waiting on a chest?
#[must_use]
pub fn waiting_on<S: Store>(s: &Server<S>, world: ConnId) -> bool {
    s.chests
        .by_id
        .values()
        .any(|c| c.waiting.as_ref().is_some_and(|w| w.world == world))
}

fn to_knowers<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, m: &ServerMessage) {
    places::to_knowers(s, net, id, m);
}

fn flash<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let m = ServerMessage::DoFx(ObjectEvent {
        sender: places::object_stamp(s, id),
        action: Fx {
            fx: look::OPENING_FX,
            scalar: 1.0,
        },
    });
    to_knowers(s, net, id, &m);
}

fn mode<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, m: u32) {
    let m = ServerMessage::DoMode(ObjectEvent {
        sender: places::object_stamp(s, id),
        action: Word(m),
    });
    to_knowers(s, net, id, &m);
}

fn shown_open<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, open: bool) {
    let m = ServerMessage::UpdateBoolVisual(VisualQuality {
        object: places::object_stamp(s, id),
        quality: q::OPEN,
        value: i32::from(open),
    });
    to_knowers(s, net, id, &m);
}

fn open_done(status: u32, chest: u64, contents: &[u64]) -> ServerMessage {
    session::call(
        function::OPEN_DONE,
        &[
            Arg::Int(status),
            id_list(&[]),
            id_list(contents),
            Arg::LongInt(chest),
        ],
    )
    .expect("an opening packs")
}

/// Does a chest's quest keep a player out: its template names a quest the player does not have under
/// way (at the phase it names, if it names one)?
fn quest_keeps_out<S: Store>(s: &Server<S>, world: ConnId, template: u32) -> bool {
    let Some(props) = s.catalog.get(template).map(|t| &t.props) else {
        return false;
    };
    let Some(quest) = props.word("Usage_RequiredQuest") else {
        return false;
    };
    let phase = props.word("Usage_RequiredQuestStatus").unwrap_or(0) & 0xFFFF;
    let held = s.players.get(&world).map(|p| p.play.quests.as_slice());
    // A status naming no phase asks only that the quest be under way.
    held.is_none_or(|q| {
        let at = crate::quests::phase_of(q, quest);
        at == 0 || (phase != 0 && at != phase)
    })
}

/// Is a use of a chest refused because its quest keeps the player out? The player is told.
fn refused_by_its_quest<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    template: u32,
) -> bool {
    if !quest_keeps_out(s, world, template) {
        return false;
    }
    // A hidden quest's refusal is the mysterious one.
    let hidden = s
        .catalog
        .get(template)
        .and_then(|t| t.props.word("Usage_RequiredQuest"))
        .is_some_and(|q| crate::quests::hidden(s, q));
    let why = if hidden {
        crate::item_use::refusal::MYSTERIOUS_USE
    } else {
        WRONG_QUEST
    };
    let no = places::use_refused(u, 0, Some(WRONG_QUEST_FLAGS), 0.0, why);
    progress::private(net, world, &no);
    true
}

/// What a chest's kind names for its use comes on its opener: an effect the install holds as it
/// holds any item's, and one it names only by what it does to a quest moves that quest.
fn effects_on_opener<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    (chest, template): (u64, u32),
    clock: Clock,
) {
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let states = s.item_uses.get(&template).cloned().unwrap_or_default();
    let moved_by_state = |s: &Server<S>, quest: u32| {
        states.iter().any(|e| {
            s.effect_table.get(e.state).is_some_and(
                |d| matches!(d.kind, crate::effect_defs::Kind::Quest { quest: q, .. } if q == quest),
            )
        })
    };
    let named: Vec<crate::quests::QuestEffect> = s
        .catalog
        .get(template)
        .map(|t| crate::quests::named_for_use(s, &t.props))
        .unwrap_or_default();
    let named: Vec<_> = named
        .into_iter()
        .filter(|e| !moved_by_state(s, e.quest))
        .collect();
    for e in named {
        crate::quests::moves(s, net, world, e, true, clock);
    }
    for e in states {
        crate::item_use::put_on(s, net, world, me, chest, e, clock);
    }
}

/// A player asks to open a chest: refused at once when its quest keeps the player out; else walked
/// to it, then opened (or refused, locked).
pub fn used<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, u: Use, clock: Clock) {
    let Some((kind, at)) = chest(s, u.item) else {
        return;
    };
    if refused_by_its_quest(s, net, world, &u, kind.template) {
        return;
    }
    let Some(here) = position(s, world) else {
        return;
    };
    if places::reach(&here, at.cell, at.origin).is_some_and(|d| d <= REACH) {
        at_chest(s, net, world, u, clock);
    } else {
        places::walk_to(s, net, world, u, REACH);
        places::arrived(s, net, world, clock);
    }
}

/// The player is at the chest: a locked one refuses (it flashes); a closed one opens.
pub fn at_chest<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let id = u.item;
    let Some((kind, at)) = chest(s, id) else {
        return;
    };
    if refused_by_its_quest(s, net, world, &u, kind.template) {
        return;
    }
    let phase = s.chests.by_id.get(&id).map(|c| c.phase).unwrap_or_default();
    if phase != Phase::Closed {
        let busy = places::use_refused(&u, 0, None, 0.0, crate::places::ACTIONS_LOCKED);
        progress::private(net, world, &busy);
        return;
    }
    if locked(s, id, &kind) {
        progress::private(net, world, &open_done(LOCKED, id, &[]));
        flash(s, net, id);
        return;
    }
    let distance = places::reach(&u.from, at.cell, at.origin).unwrap_or(0.0);
    open(
        s,
        net,
        id,
        &kind,
        Waiting {
            world,
            used: u,
            distance,
            key: None,
        },
        clock,
    );
}

/// A player uses a key it holds on a chest: walked to within three meters, then the key turns.
pub fn key_used<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let Some((_, at)) = chest(s, u.target) else {
        return;
    };
    let Some(here) = position(s, world) else {
        return;
    };
    if places::reach(&here, at.cell, at.origin).is_some_and(|d| d <= KEY_REACH) {
        key_at_chest(s, net, world, u, clock);
    } else {
        places::walk_to(s, net, world, u, KEY_REACH);
        places::arrived(s, net, world, clock);
    }
}

/// The player is at the chest with its key: a chest not locked is opened as without the key; a key
/// whose id is not the chest's is refused as the wrong key; the chest's own key turns.
pub fn key_at_chest<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let id = u.target;
    let Some((kind, at)) = chest(s, id) else {
        return;
    };
    if !locked(s, id, &kind) {
        let mut plain = u;
        plain.item = id;
        plain.target = 0;
        at_chest(s, net, world, plain, clock);
        return;
    }
    let key = s.things.get(u.item).map(|t| (t.id, t.template));
    let fits = key
        .is_some_and(|(_, t)| kind.key.is_some() && crate::usable::doors::key_id(s, t) == kind.key);
    let distance = places::reach(&u.from, at.cell, at.origin).unwrap_or(0.0);
    if !fits {
        let m = places::use_refused(
            &u,
            0,
            None,
            distance,
            crate::usable::doors::refusal::WRONG_KEY,
        );
        progress::private(net, world, &m);
        return;
    }
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    if let Some(p) = s.players.get_mut(&world) {
        let turns = ServerMessage::PlayBehavior(crate::combat::player_event(
            p,
            BehaviorParams {
                bits: BehaviorParams::BEHAVIOR | BehaviorParams::CONTEXT,
                behavior: crate::usable::doors::TURN_KEY,
                context,
                ..BehaviorParams::default()
            },
        ));
        let me = p.character;
        crate::combat::to_all_seeing(s, net, me, &turns);
    }
    let c = s.chests.by_id.entry(id).or_default();
    c.generation = c.generation.wrapping_add(1);
    let generation = c.generation;
    c.waiting = Some(Waiting {
        world,
        used: u,
        distance,
        key: key.map(|(k, _)| k),
    });
    s.after(
        clock,
        KEY_TURNING,
        Timer::Chest {
            chest: id,
            generation,
            step: Step::Turned,
        },
    );
}

/// What a chest holds at an opening: its profile's extras and its draw, at most its size; each with
/// the worth and level it holds of its own, if its profile's row says.
fn fill<S: Store>(s: &mut Server<S>, kind: &Kind) -> Vec<(u32, Option<(u32, i32)>)> {
    let Some(p) = kind
        .profile
        .and_then(|v| s.catalog.profile_name(v))
        .and_then(|n| s.loot.profile(n))
        .cloned()
    else {
        return Vec::new();
    };
    let count = s.rng.weighted(p.counts.iter().copied()).unwrap_or(0);
    let mut out = Vec::new();
    while out.len() < count && !p.items.is_empty() {
        #[allow(clippy::cast_precision_loss)]
        let Some(i) = s.rng.weighted(p.items.iter().map(|(_, w)| *w as f32)) else {
            break;
        };
        let own = p.chest.map(|c| {
            let width = c.value.1.saturating_sub(c.value.0) + 1;
            let drawn = u32::try_from(s.rng.below(width as usize)).unwrap_or(0);
            (c.value.0 + drawn, c.level)
        });
        out.push((p.items[i].0, own));
    }
    for (t, chance) in &p.extras {
        if s.rng.chance(*chance) {
            out.push((*t, None));
        }
    }
    out.truncate(kind.most.unwrap_or(usize::MAX));
    out
}

/// The chest begins to open: it flashes, what it held is cleared (for everyone who sees it, each
/// item by the chest itself: the item, then the chest), it plays its opening, and what it holds now
/// is made (2017: the flash, the clears, the opening, 26 of 26).
fn open<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    kind: &Kind,
    waiting: Waiting,
    clock: Clock,
) {
    effects_on_opener(s, net, waiting.world, (id, kind.template), clock);
    flash(s, net, id);
    let left = s
        .chests
        .by_id
        .get(&id)
        .map(|c| c.contents.clone())
        .unwrap_or_default();
    for item in left {
        if s.things.get(item).and_then(|t| t.container).map(|c| c.0) != Some(id) {
            continue;
        }
        let m = ServerMessage::VisualObjectCall(ObjectCall {
            object: places::object_stamp(s, id),
            call: ClientCall {
                function: FunctionId(DESTROY_CONTAINED),
                args: Some(
                    ScriptArgs::from_values(&[Arg::LongInt(item), Arg::LongInt(id)])
                        .expect("ids pack"),
                ),
            },
        });
        let bytes = m.encode();
        for (conn, p) in &mut s.players {
            if p.known.remove(&item).is_some() || p.known.contains_key(&id) {
                net.send(*conn, queue::EVENT, BlobKind::VISUAL, bytes.clone());
            }
        }
        s.things.remove(item);
    }
    let mut contents = Vec::new();
    for (slot, (template, own)) in fill(s, kind).into_iter().enumerate() {
        let value = own.map_or(0, |(value, _)| value);
        if let Some(item) = crate::inventory::make_item(s, template, 1, value) {
            if let Some((_, level)) = own {
                crate::treasure::endow(s, item, level, None);
            }
            if let Some(t) = s.things.get_mut(item) {
                t.container = Some((id, u32::try_from(slot).unwrap_or(0)));
            }
            contents.push(item);
        }
    }
    mode(s, net, id, look::OPEN_MODE);
    let c = s.chests.by_id.entry(id).or_default();
    c.phase = Phase::Opening;
    c.generation = c.generation.wrapping_add(1);
    c.contents = contents;
    c.waiting = Some(waiting);
    let generation = c.generation;
    s.after(
        clock,
        animation(kind.template).0,
        Timer::Chest {
            chest: id,
            generation,
            step: Step::Opened,
        },
    );
}

/// A key gone from the pack of who turned it: used on nothing, and the use refused as the wrong key.
fn refuse_gone_key(net: &mut ServerNet, w: &Waiting) {
    let no = places::use_refused(
        &w.used,
        0,
        None,
        w.distance,
        crate::usable::doors::refusal::WRONG_KEY,
    );
    progress::private(net, w.world, &no);
}

/// A chest plays its closing, whoever has it open or nobody.
fn shut<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, kind: &Kind, clock: Clock) {
    let Some(c) = s.chests.by_id.get_mut(&id) else {
        return;
    };
    c.phase = Phase::Closing;
    c.generation = c.generation.wrapping_add(1);
    let generation = c.generation;
    mode(s, net, id, look::CLOSED_MODE);
    s.after(
        clock,
        animation(kind.template).1,
        Timer::Chest {
            chest: id,
            generation,
            step: Step::Closed,
        },
    );
}

/// The client asks to close a chest it has open: it plays its closing.
pub fn close<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    id: u64,
    clock: Clock,
) {
    let Some((kind, _)) = chest(s, id) else {
        return;
    };
    let Some(c) = s.chests.by_id.get_mut(&id) else {
        return;
    };
    if c.phase != Phase::Open || c.opener != Some(world) {
        return;
    }
    shut(s, net, id, &kind, clock);
}

/// The key that turned: used up, and the use answered that it fits.
fn key_answer<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, w: &Waiting) {
    let Some(key) = w.key else { return };
    let name = presence::content_by_id(&s.content.objects, id)
        .and_then(|i| s.content.objects[i].description.name);
    let used = crate::usable::doors::fits_answer(s, &w.used, key, name);
    crate::craft::use_up(s, net, w.world, key, 1);
    let done = places::use_done(&used, 0, places::USE_FLAGS_UNLOCKED, false, w.distance);
    progress::private(net, w.world, &done);
}

/// A chest the world database's reload takes away or places again is let go: a use waiting on it is
/// refused as busy (a key used on it is kept); its opener is told it closed, and what it held goes
/// (forgotten by every client that holds it). It is closed, with no steps of its moving to come.
pub(crate) fn release<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let Some(c) = s.chests.by_id.remove(&id) else {
        return;
    };
    if let Some(w) = c.waiting {
        let busy = places::use_refused(&w.used, 0, None, w.distance, places::ACTIONS_LOCKED);
        progress::private(net, w.world, &busy);
    }
    if let Some(w) = c.opener {
        let done =
            session::call(function::CLOSE_DONE, &[Arg::Int(0), Arg::LongInt(id)]).expect("packs");
        progress::private(net, w, &done);
        if let Some(p) = s.players.get_mut(&w) {
            if p.open_container == Some(id) {
                p.open_container = None;
            }
        }
    }
    for item in c.contents {
        let Some(t) = s.things.get(item) else {
            continue;
        };
        if t.container.map(|c| c.0) != Some(id) {
            continue;
        }
        let instance = t.instance;
        presence::thing_gone(s, net, item, instance);
        s.things.remove(item);
    }
}

/// A step of a chest's moving is due (none if it has moved since).
pub fn step<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    generation: u32,
    step: Step,
    clock: Clock,
) {
    let Some((kind, _)) = chest(s, id) else {
        return;
    };
    if s.chests.by_id.get(&id).map(|c| c.generation) != Some(generation) {
        return;
    }
    match step {
        Step::Turned => {
            let Some(w) = s.chests.by_id.get_mut(&id).and_then(|c| c.waiting.take()) else {
                return;
            };
            if !crate::usable::doors::key_kept(s, w.world, w.key) {
                refuse_gone_key(net, &w);
                return;
            }
            if let Some(c) = s.chests.by_id.get_mut(&id) {
                c.unlocked = true;
            }
            if kind.opens_on_unlock {
                open(s, net, id, &kind, w, clock);
            } else {
                key_answer(s, net, id, &w);
            }
        }
        Step::Opened => {
            let Some(c) = s.chests.by_id.get_mut(&id) else {
                return;
            };
            let Some(w) = c.waiting.take() else {
                c.phase = Phase::Open;
                return;
            };
            if !crate::usable::doors::key_kept(s, w.world, w.key) {
                refuse_gone_key(net, &w);
                shut(s, net, id, &kind, clock);
                return;
            }
            // Who opened it has left the world: it shuts for the next to open.
            if !s.players.contains_key(&w.world) {
                shut(s, net, id, &kind, clock);
                return;
            }
            let Some(c) = s.chests.by_id.get_mut(&id) else {
                return;
            };
            c.phase = Phase::Open;
            c.opener = Some(w.world);
            let contents = c.contents.clone();
            progress::private(net, w.world, &open_done(0, id, &contents));
            shown_open(s, net, id, true);
            for item in &contents {
                let Some(t) = s.things.get(*item) else {
                    continue;
                };
                let m = ServerMessage::CreateObject(crate::inventory::item_create(t));
                net.send(w.world, queue::EVENT, BlobKind::UNORDERED, m.encode());
                if let Some(p) = s.players.get_mut(&w.world) {
                    p.known.insert(
                        *item,
                        presence::Seen {
                            position_stream: 0,
                            head_stream: 0,
                        },
                    );
                }
            }
            if let Some(p) = s.players.get_mut(&w.world) {
                p.open_container = Some(id);
            }
            key_answer(s, net, id, &w);
        }
        Step::Closed => {
            let Some(c) = s.chests.by_id.get_mut(&id) else {
                return;
            };
            c.phase = Phase::Closed;
            if kind.locks_on_close {
                c.unlocked = false;
            }
            let closer = c.opener.take();
            if let Some(w) = closer {
                let done = session::call(function::CLOSE_DONE, &[Arg::Int(0), Arg::LongInt(id)])
                    .expect("packs");
                progress::private(net, w, &done);
                if let Some(p) = s.players.get_mut(&w) {
                    if p.open_container == Some(id) {
                        p.open_container = None;
                    }
                }
            }
            shown_open(s, net, id, false);
        }
    }
}

/// A player has left the world: a chest it has open is shut at once, for the next to open.
pub fn left<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let open: Vec<u64> = s
        .chests
        .by_id
        .iter()
        .filter(|(_, c)| c.opener == Some(world))
        .map(|(id, _)| *id)
        .collect();
    for id in open {
        let locks = chest(s, id).is_some_and(|(kind, _)| kind.locks_on_close);
        if let Some(c) = s.chests.by_id.get_mut(&id) {
            c.opener = None;
            c.phase = Phase::Closed;
            c.generation = c.generation.wrapping_add(1);
            if locks {
                c.unlocked = false;
            }
        }
        mode(s, net, id, look::CLOSED_MODE);
        shown_open(s, net, id, false);
    }
}

/// A player has moved: a chest it has open closes once it is farther than [`WALK_AWAY`] from it.
pub fn moved<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some(here) = position(s, world) else {
        return;
    };
    let open: Vec<u64> = s
        .chests
        .by_id
        .iter()
        .filter(|(_, c)| c.phase == Phase::Open && c.opener == Some(world))
        .map(|(id, _)| *id)
        .collect();
    for id in open {
        let far = chest(s, id)
            .and_then(|(_, at)| places::reach(&here, at.cell, at.origin))
            .is_none_or(|d| d > WALK_AWAY);
        if far {
            close(s, net, world, id, clock);
        }
    }
}
