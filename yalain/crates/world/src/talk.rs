//! What people say, and when (the talk table): a person or a creature saying its lines.
//!
//! **With a quest step.** A step a person's use runs says the step's own line, then the lines the
//! person says with that update of that quest (the phase too, for an advance), in their order:
//! those heard said with it in the recordings, else the first of those its names give it.
//!
//! **Used, too low for its quest.** A person whose quest the player is under the least level of
//! says its refusal of that quest.
//!
//! **Used, too soon.** A person who gives several quests and gave the player one too lately to give
//! another says its too-soon line.
//!
//! **Given an item it has no use for.** A person says its line for that item, else its refusal of
//! a give (else another line it says at a give naming no quest) as it hands the item back.
//!
//! **Given an item it takes, no quest moved.** A person says its lines for taking that item, else
//! one of its lines for taking any item.
//!
//! **Given a map.** A person who sends players where their maps lead says its line for that as it
//! hands the map back, and its own line for a map it does not take.
//!
//! **Used, with nothing for the player.** A storekeeper greets as its store row says; anyone
//! else (a storekeeper without a greeting too) says a reminder of one of its quests the player has
//! under way, else one of the lines it says when used (refusals, its own words at a use), picked
//! at random, heard ones first. Never its too-soon line, which is for when it is too soon.
//!
//! **Unprompted.** A person with lines for a player near says one to each player who comes within
//! [`NEAR`] of it, at most once in [`NEAR_AGAIN`]; one with idle lines says one every
//! [`IDLE_EVERY`] while a player is within [`IDLE_HEARD`], and one whose idle lines are broadcast
//! one every [`BROADCAST_EVERY`], a player near or not. A creature with fighting lines says one as
//! it goes after a player, and one with dying lines one as it dies.
//!
//! **How.** A tell reaches the player, naming the speaker; a saying is heard by every client that
//! knows the speaker; an emote is text in the player's chat window, which names the speaker
//! itself; a broadcast reaches every player in the speaker's landblock and the eight around it
//! (indoors, its own landblock), known to their clients or not. A string's lines take the player's
//! name as their variable, except a broadcast said of itself, which names no one; literal text goes
//! as it is.

use auberean_protocol::script::{ClientCall, FunctionId, ObjectCall, ScriptArgs};
use auberean_protocol::types::ObjectStamp;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::TextInfo;
use yalain_content::quests::Update;
use yalain_content::talk::{How, Line, Role, TalkUpdate, When, Words};
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::{chat, presence, progress, quests, Clock, Server};

/// How near a player comes before a person with lines for it greets it, in meters (authored).
pub const NEAR: f32 = 10.0;

/// How long before a person greets the same player again, in seconds (authored).
pub const NEAR_AGAIN: f64 = 300.0;

/// How near a player must be for a person to chatter, in meters (authored).
pub const IDLE_HEARD: f32 = 20.0;

/// The shortest and longest wait between a person's idle lines, in seconds (authored).
pub const IDLE_EVERY: (f64, f64) = (60.0, 120.0);

/// The shortest and longest wait between a broadcaster's idle lines, in seconds (2017: 52 of 61
/// gaps 20.1 to 49.8 s, evenly spread).
pub const BROADCAST_EVERY: (f64, f64) = (20.0, 50.0);

/// The call a broadcast line travels as, and the kind of line it carries.
pub const BROADCAST: u32 = 0x8205_001F;
pub const BROADCAST_KIND: u32 = 256;

/// The kind of text an emote is shown as in the chat window: one naming its speaker.
pub const EMOTE_TEXT: u32 = 8;

/// Who says a line: its object, its name, and its object's stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Speaker {
    pub id: u64,
    pub name: Option<(u32, u32)>,
    /// The name it goes by as literal text, in place of `name`.
    pub name_text: Option<String>,
    pub stamp: ObjectStamp,
}

impl Speaker {
    /// Its name as a player reads it.
    #[must_use]
    pub fn told_as(&self) -> TextInfo {
        match &self.name_text {
            Some(n) => text_for(&Words::Literal(n.clone()), ""),
            None => self
                .name
                .map_or_else(TextInfo::default, crate::progress::text),
        }
    }
}

/// A content object as a speaker.
#[must_use]
pub fn content_speaker<S: Store>(s: &Server<S>, i: usize) -> Speaker {
    let o = &s.content.objects[i];
    let id = presence::content_id(&s.content.objects, i);
    Speaker {
        id,
        name: o.description.name.or(o.facts.name),
        name_text: o.description.name_text.clone(),
        stamp: ObjectStamp {
            id,
            instance_stamp: s.placed.instance(id),
            other_stamp: s.visuals.get(&id).copied().unwrap_or(1),
        },
    }
}

/// A thing of the world (a creature) as a speaker.
#[must_use]
pub fn thing_speaker<S: Store>(s: &Server<S>, id: u64) -> Option<Speaker> {
    let t = s.things.get(id)?;
    Some(Speaker {
        id,
        name: t.facts.name,
        name_text: None,
        stamp: ObjectStamp {
            id,
            instance_stamp: t.instance,
            other_stamp: t.visual,
        },
    })
}

/// The words as a player reads them: a string with the player's name as its variable, or the
/// literal text.
#[must_use]
pub fn text_for(words: &Words, player_world_name: &str) -> TextInfo {
    match words {
        Words::Ref(string, table) => quests::line_to((*string, *table), player_world_name),
        Words::Literal(t) => TextInfo {
            has_literal: 1,
            literal: Some(t.encode_utf16().collect()),
            ..TextInfo::default()
        },
    }
}

/// Whether a player in cell `heard` hears a broadcast from cell `from`: outdoors, the speaker's
/// landblock and the eight around it; indoors, the speaker's landblock alone.
#[must_use]
pub fn hears_broadcast(from: u32, heard: u32) -> bool {
    let indoors = |c: u32| c & 0xFFFF >= 0x100;
    if indoors(from) || indoors(heard) {
        return from >> 16 == heard >> 16;
    }
    let (fx, fy) = ((from >> 24) & 0xFF, (from >> 16) & 0xFF);
    let (hx, hy) = ((heard >> 24) & 0xFF, (heard >> 16) & 0xFF);
    fx.abs_diff(hx) <= 1 && fy.abs_diff(hy) <= 1
}

/// The cell a speaker stands in: a content object's or a thing's.
fn speaker_cell<S: Store>(s: &Server<S>, id: u64) -> Option<u32> {
    if let Some(t) = s.things.get(id) {
        return t.at.map(|p| p.cell);
    }
    s.content
        .objects
        .iter()
        .enumerate()
        .find(|(i, _)| presence::content_id(&s.content.objects, *i) == id)
        .map(|(_, o)| o.cell)
}

/// A speaker says words to the player of `world`, as `how` says.
pub fn say<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    speaker: &Speaker,
    words: &Words,
    how: How,
) {
    match how {
        How::Tell => quests::tell(s, net, world, speaker.id, speaker.told_as(), words),
        How::Emote => {
            let Some(p) = s.players.get(&world) else {
                return;
            };
            let text = text_for(words, &crate::player::world_name(&p.name, p.sex));
            progress::private(net, world, &chat::show_text(EMOTE_TEXT, text));
        }
        How::Broadcast => {
            let named = s
                .players
                .get(&world)
                .map(|p| crate::player::world_name(&p.name, p.sex));
            broadcast(s, net, speaker, words, named.as_deref());
        }
        How::Say => {
            for (conn, p) in &s.players {
                if *conn != world && !p.known.contains_key(&speaker.id) {
                    continue;
                }
                let text = text_for(words, &crate::player::world_name(&p.name, p.sex));
                let Ok(args) = ScriptArgs::from_values(&[Arg::Int(0), chat::text_arg(text)]) else {
                    continue;
                };
                let m = ServerMessage::CellObjectCall(ObjectCall {
                    object: speaker.stamp,
                    call: ClientCall {
                        function: FunctionId(chat::function::SAYING),
                        args: Some(args),
                    },
                });
                net.send(*conn, queue::EVENT, BlobKind::UNORDERED, m.encode());
            }
        }
    }
}

/// A speaker's line reaches everyone who hears a broadcast from it, known to their clients or not.
/// A line a player's act brought on names that player as its variable; one said of itself names no
/// one.
fn broadcast<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    speaker: &Speaker,
    words: &Words,
    named: Option<&str>,
) {
    let Some(at) = speaker_cell(s, speaker.id) else {
        return;
    };
    let text = match (words, named) {
        (Words::Ref(string, table), None) => progress::text((*string, *table)),
        (_, name) => text_for(words, name.unwrap_or_default()),
    };
    let Ok(args) = ScriptArgs::from_values(&[Arg::Int(BROADCAST_KIND), chat::text_arg(text)])
    else {
        return;
    };
    let m = ServerMessage::CellObjectCall(ObjectCall {
        object: speaker.stamp,
        call: ClientCall {
            function: FunctionId(BROADCAST),
            args: Some(args),
        },
    });
    for (conn, p) in &s.players {
        if hears_broadcast(at, p.position.cell) {
            net.send(*conn, queue::EVENT, BlobKind::UNORDERED, m.encode());
        }
    }
}

/// The quest update a step makes, as a line names it.
#[must_use]
pub const fn update_of(u: Update) -> (TalkUpdate, Option<u32>) {
    match u {
        Update::Bestow => (TalkUpdate::Bestow, None),
        Update::Advance(p) => (TalkUpdate::Advance, Some(p)),
        Update::Complete => (TalkUpdate::Complete, None),
        // No line names none: nothing is said for it, the phase below being no phase.
        Update::Fail | Update::Nothing => (TalkUpdate::Fail, None),
    }
}

/// The lines a person says with a quest update one of its steps made: those heard said with it,
/// in their order; else the first of the others.
#[must_use]
pub fn with_update<S: Store>(
    s: &Server<S>,
    who: u32,
    object: Option<usize>,
    quest: u32,
    update: Update,
) -> Vec<Line> {
    if update == Update::Nothing {
        return Vec::new();
    }
    let (up, phase) = update_of(update);
    let mut lines: Vec<&Line> = s
        .talk
        .of(who, object)
        .filter(|l| {
            l.quest == Some(quest)
                && l.update == Some(up)
                && (phase.is_none() || l.phase.is_none() || l.phase == phase)
        })
        .collect();
    if lines.iter().any(|l| l.measured) {
        lines.retain(|l| l.measured);
    } else {
        lines.truncate(1);
    }
    lines.sort_by_key(|l| l.order);
    lines.into_iter().cloned().collect()
}

/// One of a pool of lines, picked at random, heard ones first.
fn pick<S: Store>(s: &mut Server<S>, pool: &[Line]) -> Option<Line> {
    let heard: Vec<&Line> = pool.iter().filter(|l| l.measured).collect();
    let from: Vec<&Line> = if heard.is_empty() {
        pool.iter().collect()
    } else {
        heard
    };
    let i = s.rng.below(from.len());
    from.get(i).map(|l| (*l).clone())
}

/// What a person who gives several quests says when it gave one too lately to give another: its
/// too-soon line (2017: Stubby's "Come back later ..." 18 of 18, Lupe's 7 of 7).
pub fn too_soon_line<S: Store>(s: &mut Server<S>, who: u32, object: Option<usize>) -> Option<Line> {
    let pool: Vec<Line> = s
        .talk
        .of(who, object)
        .filter(|l| l.role == Some(Role::TooSoon))
        .cloned()
        .collect();
    pick(s, &pool)
}

/// What a person used says when it would give a quest but for the player's level: its refusal of
/// that quest (2017: the Great Deru Shrine's "Your bark is too green ..." to a player under the
/// quest's least level, 1 of 1).
pub fn refusal_for<S: Store>(
    s: &mut Server<S>,
    who: u32,
    object: Option<usize>,
    quest: u32,
) -> Option<Line> {
    let pool: Vec<Line> = s
        .talk
        .of(who, object)
        .filter(|l| {
            l.when == When::Use
                && l.role == Some(Role::RefuseUse)
                && l.quest == Some(quest)
                && l.update.is_none()
        })
        .cloned()
        .collect();
    pick(s, &pool)
}

/// What a person used says when no quest step ran: a reminder of a quest of its the player has
/// under way, else one of the lines it says at a use, its too-soon line not among them (2017: 25 of
/// 25 such lines with none of the person's quests under way; with one under way, its own line 3 of
/// 3).
pub fn at_use<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    who: u32,
    object: Option<usize>,
) -> Option<Line> {
    let held = s.players.get(&world).map(|p| p.play.quests.clone())?;
    let lines: Vec<Line> = s
        .talk
        .of(who, object)
        .filter(|l| l.when == When::Use && l.update.is_none())
        .cloned()
        .collect();
    let reminders: Vec<Line> = lines
        .iter()
        .filter(|l| {
            l.role == Some(Role::Remind)
                && l.quest.is_some_and(|q| {
                    let at = quests::phase_of(&held, q);
                    at > 0 && l.phase.is_none_or(|p| p == at)
                })
        })
        .cloned()
        .collect();
    if !reminders.is_empty() {
        return pick(s, &reminders);
    }
    let said: Vec<Line> = lines
        .into_iter()
        .filter(|l| !matches!(l.role, Some(Role::Remind | Role::TooSoon)) && l.quest.is_none())
        .collect();
    pick(s, &said)
}

/// What a person given an item it has no step for says as it hands the item back: a line naming
/// that item, else its refusal of a give, else another of the lines it says when given something
/// that names no quest (2017: 30 of 30 such gives said one of the person's own lines at a give, 13
/// of them its refusal). A line naming an item is said of that item and no other, and a person's
/// lines for taking an item are not among these.
pub fn at_give<S: Store>(
    s: &mut Server<S>,
    who: u32,
    object: Option<usize>,
    item: u32,
) -> Option<Line> {
    let lines: Vec<Line> = s
        .talk
        .of(who, object)
        .filter(|l| {
            l.when == When::Give
                && l.quest.is_none()
                && l.update.is_none()
                && l.item.is_none_or(|i| i == item)
                && l.role != Some(Role::Accept)
        })
        .cloned()
        .collect();
    let named: Vec<Line> = lines
        .iter()
        .filter(|l| l.item == Some(item) && !matches!(l.role, Some(Role::Teleport)))
        .cloned()
        .collect();
    if !named.is_empty() {
        return pick(s, &named);
    }
    let refusals: Vec<Line> = lines
        .iter()
        .filter(|l| l.role == Some(Role::RefuseGive))
        .cloned()
        .collect();
    if !refusals.is_empty() {
        return pick(s, &refusals);
    }
    let others: Vec<Line> = lines
        .into_iter()
        .filter(|l| matches!(l.role, None | Some(Role::Use | Role::Remind)))
        .collect();
    pick(s, &others)
}

/// What a person says when given the wrong count of an item a quest under way wants: its line for
/// that (a `refuse-give` line naming the quest and the item and no update), else its plain refusal
/// (2017: Lupe, given three fins she wanted one at a time, her plain refusal, 1 of 1).
pub fn at_wrong_count<S: Store>(
    s: &mut Server<S>,
    who: u32,
    object: Option<usize>,
    quest: u32,
    item: u32,
) -> Option<Line> {
    let lines: Vec<Line> = s
        .talk
        .of(who, object)
        .filter(|l| {
            l.when == When::Give
                && l.role == Some(Role::RefuseGive)
                && l.quest == Some(quest)
                && l.item == Some(item)
                && l.update.is_none()
        })
        .cloned()
        .collect();
    if lines.is_empty() {
        return at_give(s, who, object, item);
    }
    pick(s, &lines)
}

/// What a person says as it takes an item by a give that moves no quest: its lines for taking that
/// item, in their order, else one of its lines for taking any item.
pub fn with_give<S: Store>(
    s: &mut Server<S>,
    who: u32,
    object: Option<usize>,
    item: u32,
) -> Vec<Line> {
    let taking: Vec<Line> = s
        .talk
        .of(who, object)
        .filter(|l| {
            l.when == When::Give
                && l.role == Some(Role::Accept)
                && l.quest.is_none()
                && l.update.is_none()
        })
        .cloned()
        .collect();
    let mut named: Vec<Line> = taking
        .iter()
        .filter(|l| l.item == Some(item))
        .cloned()
        .collect();
    if named.is_empty() {
        let any: Vec<Line> = taking.into_iter().filter(|l| l.item.is_none()).collect();
        return pick(s, &any).into_iter().collect();
    }
    named.sort_by_key(|l| l.order);
    named
}

/// A person's line of a role said at a give, picked at random (heard ones first): of those naming
/// no item or the item given, else of them all.
pub fn at_give_as<S: Store>(
    s: &mut Server<S>,
    who: u32,
    object: Option<usize>,
    role: Role,
    item: u32,
) -> Option<Line> {
    let lines: Vec<Line> = s
        .talk
        .of(who, object)
        .filter(|l| l.when == When::Give && l.role == Some(role))
        .cloned()
        .collect();
    let fits: Vec<Line> = lines
        .iter()
        .filter(|l| l.item.is_none_or(|i| i == item))
        .cloned()
        .collect();
    pick(s, if fits.is_empty() { &lines } else { &fits })
}

/// Whether using an object of a template is talking to it: it says something when used.
#[must_use]
pub fn talks_when_used<S: Store>(s: &Server<S>, template: u32) -> bool {
    s.talk.says_at(template, When::Use)
}

/// A creature goes after a player: one of its fighting lines, to that player.
pub fn fights<S: Store>(s: &mut Server<S>, net: &mut ServerNet, creature: u64, player: u64) {
    say_one_of(s, net, creature, player, When::Attack);
}

/// A creature dies by a player's blow: one of its dying lines, to that player.
pub fn dies<S: Store>(s: &mut Server<S>, net: &mut ServerNet, creature: u64, killer: u64) {
    say_one_of(s, net, creature, killer, When::Death);
}

fn say_one_of<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    creature: u64,
    player: u64,
    when: When,
) {
    let Some(template) = s.things.get(creature).map(|t| t.template) else {
        return;
    };
    if !s.talk.says_at(template, when) {
        return;
    }
    let Some(world) = s
        .players
        .iter()
        .find(|(_, p)| p.character == player)
        .map(|(w, _)| *w)
    else {
        return;
    };
    let pool: Vec<Line> = s
        .talk
        .of(template, None)
        .filter(|l| l.when == when)
        .cloned()
        .collect();
    let (Some(line), Some(speaker)) = (pick(s, &pool), thing_speaker(s, creature)) else {
        return;
    };
    say(s, net, world, &speaker, &line.words, line.how);
}

/// The world's step for what people say unprompted: greetings to players come near, and
/// chatter.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    if s.talk.lines.is_empty() || s.players.is_empty() {
        return;
    }
    let now = clock.local.0;
    if s.talkers.is_none() {
        let mut near = Vec::new();
        let mut idle = Vec::new();
        for (i, o) in s.content.objects.iter().enumerate() {
            if s.talk.says_at(o.template, When::Near) {
                near.push(i);
            }
            if s.talk.says_at(o.template, When::Idle) {
                idle.push(i);
            }
        }
        s.talkers = Some((near, idle));
    }
    let (near, idle) = s.talkers.clone().unwrap_or_default();
    let players: Vec<(ConnId, u64, yalain_store::StoredPosition)> = s
        .players
        .iter()
        .filter(|(_, p)| p.scene_drawn && p.trip.is_none() && p.dying.is_none())
        .map(|(c, p)| (*c, p.character, p.position))
        .collect();
    for i in near {
        let o = &s.content.objects[i];
        let at = yalain_store::StoredPosition {
            cell: o.cell,
            origin: o.origin,
            rotation: o.rotation,
        };
        let template = o.template;
        for (world, character, pos) in &players {
            if !presence::distance(pos, &at).is_some_and(|d| d <= NEAR) {
                continue;
            }
            let last = s.greeted.get(&(*character, i)).copied();
            if last.is_some_and(|t| now - t < NEAR_AGAIN) {
                continue;
            }
            s.greeted.insert((*character, i), now);
            let pool: Vec<Line> = s
                .talk
                .of(template, Some(i))
                .filter(|l| l.when == When::Near)
                .cloned()
                .collect();
            if let Some(line) = pick(s, &pool) {
                let speaker = content_speaker(s, i);
                say(s, net, *world, &speaker, &line.words, line.how);
            }
        }
    }
    for i in idle {
        let o = &s.content.objects[i];
        let at = yalain_store::StoredPosition {
            cell: o.cell,
            origin: o.origin,
            rotation: o.rotation,
        };
        let template = o.template;
        let pool: Vec<Line> = s
            .talk
            .of(template, Some(i))
            .filter(|l| l.when == When::Idle)
            .cloned()
            .collect();
        // A broadcaster's idle lines run on their own timer, a player near or not.
        let broadcasts = pool.iter().all(|l| l.how == How::Broadcast);
        let world = if broadcasts {
            players.first().map(|(w, _, _)| *w)
        } else {
            players
                .iter()
                .find(|(_, _, pos)| presence::distance(pos, &at).is_some_and(|d| d <= IDLE_HEARD))
                .map(|(w, _, _)| *w)
        };
        let Some(world) = world else {
            continue;
        };
        let due = s.chatter.get(&i).copied();
        if due.is_some_and(|t| now < t) {
            continue;
        }
        let every = if broadcasts {
            BROADCAST_EVERY
        } else {
            IDLE_EVERY
        };
        let wait = every.0 + (every.1 - every.0) * f64::from(s.rng.unit());
        s.chatter.insert(i, now + wait);
        if due.is_none() {
            // The first time a player is in earshot: a wait before the first line.
            continue;
        }
        if let Some(line) = pick(s, &pool) {
            let speaker = content_speaker(s, i);
            if line.how == How::Broadcast {
                broadcast(s, net, &speaker, &line.words, None);
            } else {
                say(s, net, world, &speaker, &line.words, line.how);
            }
        }
    }
}
