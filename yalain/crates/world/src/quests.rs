//! Quests: a step of a quest is triggered by using a person, an object or a held item (or a held
//! item on an object), giving an item to a person or an object, arriving through a portal, taking an item, killing a creature or completing
//! another quest, when the
//! player's quest is at the phase the step needs and the other quests it requires are where it
//! requires them. A step that gives a quest needs it not under way, and not done unless its wait
//! to be taken again is over (a quest without one is done once). One event runs every step it
//! triggers, each quest moved on at most once, a step that another of the same event made possible
//! included (an explorer completes one quest and gives the next). The person used says its line
//! (a tell naming the player), the step's items are given and taken, then the quest is updated:
//! given (under way at its first phase), moved on to a phase, or completed (its experience paid
//! first, weighed by the quest's challenge against the player's level as a kill's is). A step that
//! takes the player somewhere makes its update when the player arrives. Each update sends the quest's record as the
//! journal shows it: its name, description and icon, status and phase, the phase's journal entry,
//! its challenge, when it was given and last done, whether and when it expires, when it may be
//! taken again. A character entering the world is sent every quest it holds, under way or not,
//! each done one with what is left of its wait to be taken again.
//!
//! **An update's effects.** A quest's definition names effects for kinds of update (a recipe
//! taught as it is given, a reward quest given as it completes, a place it sends the player); an
//! update puts them on the player before anything else of it is told.
//!
//! **Time running out.** A quest with a duration fails when its time runs out: its update of the
//! fail kind tells it failed, with the whole of its wait to be taken again. One whose time ran out
//! while its player was away is failed before the player's next entry lists it.
//!
//! **A fellowship's step.** A step may make its update for the player's fellows too: each fellow
//! in the world for whom it may run is given the same update, told it alone.
//!
//! **Subquests.** A phase of a quest may be done by doing other quests: they are given with the
//! phase, hidden, and each of their moves is told as the quest's own record, its phase kept, its
//! journal the phase's entry with those of the subquests still open; once all of them (or one,
//! where one is enough) are done, the quest moves on to its next phase.
//!
//! **Waits, levels and deaths.** A quest that failed or was cancelled is given again only once its
//! wait is over, as a done one is; one with a least level is given only to a player of that level,
//! the person saying why when it is not; one that fails when its player dies fails as the player
//! dies.
//!
//! **Hidden quests.** A quest the world keeps without showing it (its definition shows none:
//! no description, or it is set not to be shown) is held and moved on like any other, but never
//! listed at entry and never told in an update; a use that needs one and finds it not where it
//! should be is refused as mysterious (a take likewise), never as lacking a quest.

use auberean_script::args::Arg;
use auberean_script::native_object::{NativeObject, TextInfo};
use yalain_content::quests::{Need, QuestDef, Step, Trigger, Update};
use yalain_content::QuestTable;
use yalain_content::Words;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::graph::Builder;
use crate::progress::{self, Award};
use crate::state::QuestState;
use crate::{session, Clock, Server};

/// The client function quest updates call.
pub const UPDATE_QUEST: u32 = 0x8213_0024;

/// The native type of a quest record, and of a list of them.
pub const QUEST_INFO: u16 = 0xA0;
pub const QUEST_INFO_LIST: u16 = 0xA1;

/// Quest statuses and update kinds (the client's values).
pub mod status {
    pub const UNDERWAY: u32 = 0x1000_0000;
    pub const FAILED: u32 = 0x2000_0000;
    pub const COMPLETED: u32 = 0x3000_0000;
    pub const CANCELLED: u32 = 0x8000_0000;
}
pub mod update {
    pub const BESTOW: u32 = 0x1000_0000;
    pub const ADVANCE: u32 = 0x2000_0000;
    pub const COMPLETE: u32 = 0x3000_0000;
    pub const FAIL: u32 = 0x4000_0000;
    pub const CANCEL: u32 = 0x8000_0000;
}

/// The player's request to cancel a quest.
pub const CANCEL_QUEST: u32 = 0x8213_0027;

/// The wait a cancelled quest names before it may be taken again, in seconds: five hours for the
/// quests below, five minutes for any other (2017: one value per quest, 29 of 29 cancels).
pub const CANCEL_WAIT: f64 = 300.0;
pub const LONG_CANCEL_WAIT: f64 = 18_000.0;
pub const LONG_CANCEL_WAIT_QUESTS: [u32; 11] = [
    0x8000_00E0,
    0x8000_02D3,
    0x8000_02E3,
    0x8000_02EE,
    0x8000_02F4,
    0x8000_02FA,
    0x8000_02FB,
    0x8000_0303,
    0x8000_0338,
    0x8000_0355,
    0x8000_035A,
];

/// The variable naming the player in a person's line (a string reference whose literal is the
/// player's name in the world).
pub const PLAYER_NAME: u32 = 0x15B2_0D98;

/// The challenge a quest without one reports.
pub const NO_CHALLENGE: i32 = -999;

/// A quest's record as the journal shows it, packed.
#[must_use]
pub fn record(q: &QuestDef, st: &QuestState, game_now: f64, game_epoch: f64) -> Vec<u8> {
    write_record(q, st, None, game_now, game_epoch)
}

/// A quest's record as a player's journal shows it: a phase done by subquests shows the phase's
/// entry with the entries of those still open.
#[must_use]
pub fn record_in(
    table: &QuestTable,
    held: &[QuestState],
    q: &QuestDef,
    st: &QuestState,
    game_now: f64,
    game_epoch: f64,
) -> Vec<u8> {
    write_record(q, st, journal(table, held, q, st), game_now, game_epoch)
}

/// The journal of a phase done by subquests: the phase's own entry, and those of its subquests in
/// the reverse of their order, the last a done one's empty place, the others still open each
/// wrapping those after it (2017: 398 of 398 records of such phases; the subquests' own records
/// never sent). `None` for a phase without subquests.
#[must_use]
pub fn journal(
    table: &QuestTable,
    held: &[QuestState],
    q: &QuestDef,
    st: &QuestState,
) -> Option<TextInfo> {
    if st.status != status::UNDERWAY {
        return None;
    }
    let subs = table.subquests(q.id, st.phase)?;
    let entry = |quest: u32| -> Option<(u32, u32)> {
        let phase = phase_of(held, quest);
        let i = usize::try_from(phase).ok()?.checked_sub(1)?;
        table.quest(quest)?.journals.get(i).copied()
    };
    let mut list: Option<TextInfo> = None;
    for &sub in subs.quests.iter().rev() {
        let open = entry(sub);
        list = match (list, open) {
            (None, j) => Some(progress::text(j.unwrap_or((0, 0)))),
            (Some(l), Some(j)) => Some(progress::text_with(
                composed::PAIR,
                vec![
                    progress::nested(composed::SO_FAR, l),
                    progress::nested(composed::ENTRY, progress::text(j)),
                ],
            )),
            (l, None) => l,
        };
    }
    let own = usize::try_from(st.phase)
        .ok()
        .and_then(|p| p.checked_sub(1))
        .and_then(|i| q.journals.get(i).copied())
        .unwrap_or((0, 0));
    Some(progress::text_with(
        composed::WHOLE,
        vec![
            progress::nested(composed::LIST, list.unwrap_or_default()),
            progress::nested(composed::ENTRY, progress::text(own)),
        ],
    ))
}

/// The strings a phase's journal with subquests is put together with, and their variables.
pub mod composed {
    /// The whole: the subquests' entries, then the phase's own.
    pub const WHOLE: (u32, u32) = (0xC15A_CF05, 0x2500_07FA);
    /// Two entries joined: those so far, then one more.
    pub const PAIR: (u32, u32) = (0x1359_8954, 0x2500_07FA);
    pub const LIST: u32 = 0x163F_C1BC;
    pub const SO_FAR: u32 = 0x9D16_C540;
    pub const ENTRY: u32 = 0xB000_0000;
}

/// The wait before a quest no longer under way may be taken again, in seconds: a cancelled one's
/// is its cancel wait, a done or failed one's the quest's own (2017: at entry, what was left of
/// them agrees for every record of a quest whose wait is known: 13,348 done, 1,424 failed, 1,423
/// cancelled).
#[must_use]
pub fn wait_after(q: &QuestDef, status: u32) -> f64 {
    if status == status::CANCELLED {
        if LONG_CANCEL_WAIT_QUESTS.contains(&q.id) {
            LONG_CANCEL_WAIT
        } else {
            CANCEL_WAIT
        }
    } else {
        q.retry
    }
}

/// A quest's record, its journal entry given where it is not the phase's.
fn write_record(
    q: &QuestDef,
    st: &QuestState,
    journal: Option<TextInfo>,
    game_now: f64,
    game_epoch: f64,
) -> Vec<u8> {
    let underway = st.status == status::UNDERWAY;
    let mut b = Vec::new();
    let w32 = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
    let f64w = |b: &mut Vec<u8>, v: f64| b.extend_from_slice(&v.to_bits().to_le_bytes());
    let ti = |b: &mut Vec<u8>, (s, t): (u32, u32)| {
        b.extend_from_slice(
            &TextInfo {
                string_id: s,
                table: t,
                ..TextInfo::default()
            }
            .pack(),
        );
    };
    w32(&mut b, q.id);
    ti(&mut b, q.name);
    ti(&mut b, q.description);
    w32(&mut b, q.icon);
    w32(&mut b, st.status);
    w32(&mut b, if underway { st.phase } else { 0 });
    if let Some(j) = journal {
        b.extend_from_slice(&j.pack());
    } else {
        let own = if underway {
            usize::try_from(st.phase)
                .ok()
                .and_then(|p| p.checked_sub(1))
                .and_then(|i| q.journals.get(i).copied())
                .unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        ti(&mut b, own);
    }
    w32(&mut b, q.challenge.cast_unsigned());
    #[allow(clippy::cast_precision_loss)]
    let at = |unix: i64| unix as f64 + game_epoch;
    f64w(&mut b, at(st.started));
    // When it was last done: the completion's time once done, else the clock's zero.
    f64w(&mut b, if underway { game_epoch } else { at(st.updated) });
    let expires = underway && q.duration > 0.0;
    w32(&mut b, u32::from(expires));
    f64w(
        &mut b,
        if expires {
            (q.duration - (game_now - at(st.started))).max(0.0)
        } else {
            -1.0
        },
    );
    // The wait to take it again: what is left of it (the whole wait the moment it is done).
    let left = || (wait_after(q, st.status) - (game_now - at(st.updated))).max(0.0);
    f64w(&mut b, if underway { 0.0 } else { left() });
    w32(&mut b, u32::from(q.play_fx));
    w32(&mut b, u32::from(q.cancellable));
    b
}

/// The update call: the result (0), the record, the update kind.
#[must_use]
pub fn update_call(record: Vec<u8>, kind: u32) -> auberean_protocol::ServerMessage {
    let mut b = Builder::new();
    b.native(
        QUEST_INFO,
        NativeObject::Packed {
            ty: QUEST_INFO.into(),
            bytes: record,
        },
    );
    session::call(
        UPDATE_QUEST,
        &[Arg::Int(0), Arg::Object(b.graph()), Arg::Int(kind)],
    )
    .expect("a quest record packs")
}

/// The quest list the session start carries: every quest the player holds, under way, done,
/// failed or cancelled (2017: 35,691 records at 433 entries, 33,562 of them not under way; no
/// quest a list held was missing from the same character's next list, 33,142 of 33,142), but for
/// the hidden ones (none of the 289 hidden quests in any list or update), packed.
#[must_use]
pub fn list(
    s_quests: &yalain_content::QuestTable,
    held: &[QuestState],
    game_now: f64,
    game_epoch: f64,
) -> Vec<u8> {
    let mine: Vec<(&QuestDef, &QuestState)> = held
        .iter()
        .filter_map(|st| s_quests.quest(st.quest).map(|d| (d, st)))
        .filter(|(d, _)| !is_hidden(s_quests, d))
        .collect();
    let mut out = u32::try_from(mine.len())
        .unwrap_or(0)
        .to_le_bytes()
        .to_vec();
    for (d, st) in mine {
        out.extend(record_in(s_quests, held, d, st, game_now, game_epoch));
    }
    out
}

/// A person's line to a player: its string, with the player's name as the variable the lines use.
#[must_use]
pub fn line_to(line: (u32, u32), player_world_name: &str) -> TextInfo {
    let mut t = progress::text(line);
    t.set_variable(progress::nested(
        PLAYER_NAME,
        TextInfo {
            has_literal: 1,
            literal: Some(player_world_name.encode_utf16().collect()),
            ..TextInfo::default()
        },
    ));
    t
}

/// A person tells the player something: a string of its, or literal text.
pub fn tell<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    teller: u64,
    teller_name: Option<(u32, u32)>,
    line: &Words,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let msg = crate::talk::text_for(line, &crate::player::world_name(&p.name, p.sex));
    let m = session::call(
        crate::chat::function::HEAR_TELL,
        &[
            Arg::Int(0),
            crate::chat::text_arg(msg),
            crate::chat::text_arg(teller_name.map_or_else(TextInfo::default, progress::text)),
            Arg::LongInt(teller),
        ],
    )
    .expect("a tell packs");
    progress::private(net, world, &m);
}

/// The phase a player's quest is at: 0 when it is not under way.
#[must_use]
pub fn phase_of(held: &[QuestState], quest: u32) -> u32 {
    held.iter()
        .find(|q| q.quest == quest && q.status == status::UNDERWAY)
        .map_or(0, |q| q.phase)
}

/// Is a quest done for a player (completed)?
#[must_use]
pub fn done(held: &[QuestState], quest: u32) -> bool {
    held.iter()
        .any(|q| q.quest == quest && q.status == status::COMPLETED)
}

/// May a step run for a player now: its quest at the phase it needs (to be given: not under way,
/// its wait to be taken again over, the player of its least level), the quests it requires where
/// it requires them, and the items it takes held.
fn may_run<S: Store>(s: &Server<S>, world: ConnId, st: &Step, now_unix: i64) -> bool {
    may_run_at_any_level(s, world, st, now_unix) && level_allows(s, world, st)
}

/// May a step run for a player now (a step due after a wait runs only if it still may).
#[must_use]
pub fn may_run_now<S: Store>(s: &Server<S>, world: ConnId, st: &Step, now_unix: i64) -> bool {
    may_run(s, world, st, now_unix)
}

/// Is the player of the level a step that gives a quest needs (the quest's least level)?
fn level_allows<S: Store>(s: &Server<S>, world: ConnId, st: &Step) -> bool {
    if st.needs != 0 || st.update != Update::Bestow {
        return true;
    }
    let least = s.quests.quest(st.quest).map_or(0, |d| d.min_level);
    s.players
        .get(&world)
        .is_some_and(|p| crate::examine::level_of(&p.qualities) >= least)
}

/// May a step run for a player now, its quest's least level aside.
fn may_run_at_any_level<S: Store>(s: &Server<S>, world: ConnId, st: &Step, now_unix: i64) -> bool {
    let Some(p) = s.players.get(&world) else {
        return false;
    };
    let held = &p.play.quests;
    if phase_of(held, st.quest) != st.needs {
        return false;
    }
    if !st.races.is_empty() && !st.races.contains(&p.race) {
        return false;
    }
    if st.needs == 0 {
        let Some(def) = s.quests.quest(st.quest) else {
            return false;
        };
        if !given_again(def, held, now_unix) {
            return false;
        }
    }
    let requires = st.requires.iter().all(|(q, need)| match need {
        Need::Phase(n) => phase_of(held, *q) == *n,
        Need::Done => done(held, *q),
    });
    requires
        && st
            .takes
            .iter()
            .all(|t| crate::inventory::holds(s, world, *t))
}

/// May a quest a player does not have under way be given it again: never had, or its wait over
/// (a done one's the quest's own, none meaning done once; a failed one's the same; a cancelled
/// one's its cancel wait). 2017: a failed quest's shrine said only its refusal while the wait
/// ran, 2 of 2.
fn given_again(def: &QuestDef, held: &[QuestState], now_unix: i64) -> bool {
    held.iter().find(|q| q.quest == def.id).is_none_or(|q| {
        #[allow(clippy::cast_precision_loss)]
        let waited = (now_unix - q.updated) as f64;
        let wait = wait_after(def, q.status);
        match q.status {
            status::COMPLETED => wait > 0.0 && waited >= wait,
            status::FAILED | status::CANCELLED => waited >= wait,
            _ => true,
        }
    })
}

/// The quest a trigger would give a player but for the player's level (it is under the quest's
/// least level), if any.
#[must_use]
pub fn too_low<S: Store>(
    s: &Server<S>,
    world: ConnId,
    trigger: Trigger,
    now_unix: i64,
) -> Option<u32> {
    let p = s.players.get(&world)?;
    let level = crate::examine::level_of(&p.qualities);
    s.quests
        .steps
        .iter()
        .filter(|st| st.trigger == trigger && st.needs == 0 && st.update == Update::Bestow)
        .find(|st| {
            s.quests
                .quest(st.quest)
                .is_some_and(|d| level < d.min_level && phase_of(&p.play.quests, d.id) == 0)
                && may_run_at_any_level(s, world, st, now_unix)
        })
        .map(|st| st.quest)
}

/// Is a quest one its player is never told of: hidden, or a subquest of another.
#[must_use]
pub fn is_hidden(table: &QuestTable, q: &QuestDef) -> bool {
    q.hidden || table.parents(q.id).next().is_some()
}

/// Something happened that may move quests on: run every step it triggers (each quest at most
/// once). Returns whether one ran. `who` is the person or object used (for its line), with its
/// name.
pub fn triggered<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    trigger: Trigger,
    who: Option<(u64, Option<(u32, u32)>)>,
    clock: Clock,
) -> bool {
    triggered_with(s, net, world, trigger, who, None, clock).0
}

/// An item given to a person or an object may move quests on: every step the give triggers runs
/// (each quest at most once), a step that takes the item keeping the one given, the items a step
/// gives made from the person. An item so given that moves a quest when received moves it after.
/// Returns whether a step kept the item given.
pub fn triggered_by_give<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    trigger: Trigger,
    who: Option<(u64, Option<(u32, u32)>)>,
    given: crate::gives::Given,
    clock: Clock,
) -> bool {
    triggered_with(s, net, world, trigger, who, Some(given), clock).1
}

fn triggered_with<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    trigger: Trigger,
    who: Option<(u64, Option<(u32, u32)>)>,
    mut given: Option<crate::gives::Given>,
    clock: Clock,
) -> (bool, bool) {
    let mut moved: Vec<u32> = Vec::new();
    let mut kept = false;
    loop {
        let Some(step) = s
            .quests
            .steps
            .iter()
            .find(|st| {
                st.trigger == trigger
                    && st.to.is_none()
                    && !moved.contains(&st.quest)
                    && may_run(s, world, st, clock.unix)
            })
            .cloned()
        else {
            return (!moved.is_empty(), kept);
        };
        moved.push(step.quest);
        let received = run_with(s, net, world, &step, who, given, clock);
        // The item given is kept by the first step that takes it.
        if given.is_some_and(|g| step.takes.contains(&g.template)) {
            given = None;
            kept = true;
        }
        for template in received {
            triggered(s, net, world, Trigger::Take(template), None, clock);
        }
    }
}

/// The step a trigger would run now, if any (a step that takes the player somewhere runs only so:
/// its trip is made first, its update on arrival).
#[must_use]
pub fn step_for<S: Store>(
    s: &Server<S>,
    world: ConnId,
    trigger: Trigger,
    now_unix: i64,
) -> Option<Step> {
    s.quests
        .steps
        .iter()
        .find(|st| st.trigger == trigger && may_run(s, world, st, now_unix))
        .cloned()
}

/// Run a step: the line and what else the person says with the update, the items given (so many
/// of each) and taken, its effects on the player and their text, the update (for some steps the
/// update, then the effects).
pub fn run<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    step: &Step,
    who: Option<(u64, Option<(u32, u32)>)>,
    clock: Clock,
) {
    run_with(s, net, world, step, who, None, clock);
}

/// Run a step, for an item given (if any): the item kept where the step takes its template, the
/// step's items made from the person given it. Returns the templates of the items a give's step
/// gave.
fn run_with<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    step: &Step,
    who: Option<(u64, Option<(u32, u32)>)>,
    given: Option<crate::gives::Given>,
    clock: Clock,
) -> Vec<u32> {
    if let Some((teller, name)) = who {
        if let Some(line) = &step.tell {
            tell(s, net, world, teller, name, line);
        }
        if let Some(i) = crate::presence::content_by_id(&s.content.objects, teller) {
            let template = s.content.objects[i].template;
            let speaker = crate::talk::content_speaker(s, i);
            for l in crate::talk::with_update(s, template, Some(i), step.quest, step.update) {
                if step.tell.as_ref() != Some(&l.words) {
                    crate::talk::say(s, net, world, &speaker, &l.words, l.how);
                }
            }
        }
    }
    let mut received = Vec::new();
    let mut kept = given.filter(|g| step.takes.contains(&g.template));
    if let Some(g) = kept {
        crate::gives::keep(s, net, world, g);
    }
    for (i, g) in step.gives.iter().enumerate() {
        let n = step.counts.get(i).copied().unwrap_or(1).max(1);
        match given {
            Some(from) => {
                crate::inventory::give_from(s, net, world, *g, n, from.target);
                received.push(*g);
            }
            None => {
                crate::inventory::give(s, net, world, *g, n);
            }
        }
    }
    for t in &step.takes {
        if kept.is_some_and(|g| g.template == *t) {
            kept = None;
            continue;
        }
        crate::inventory::take_from_pack(s, net, world, *t);
    }
    // Its effects come before the update, or for some steps after it.
    if step.effects_last {
        apply(s, net, world, step.quest, step.update, step.xp, clock);
        step_effects(s, net, world, step, who, clock);
    } else {
        step_effects(s, net, world, step, who, clock);
        apply(s, net, world, step.quest, step.update, step.xp, clock);
    }
    if step.fellows {
        for_fellows(s, net, world, step, clock);
    }
    received
}

/// A step whose update is the fellowship's: the same update made for each of the player's fellows
/// in the world for whom the step may run, told to each alone (2017: a fellow's hoard key gave the
/// hoard's entry quest to the fellow who recorded, an update and nothing else).
fn for_fellows<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    step: &Step,
    clock: Clock,
) {
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(f) = s.fellowships.of(me) else {
        return;
    };
    let fellows: Vec<ConnId> = s.fellowships.list[f]
        .fellows
        .iter()
        .filter(|x| x.id != me)
        .filter_map(|x| crate::social::world_of(s, x.id))
        .collect();
    for w in fellows {
        if may_run(s, w, step, clock.unix) {
            apply(s, net, w, step.quest, step.update, 0, clock);
        }
    }
}

/// A step's effects on the player (a reward, a recipe learned), cast by the player itself, then
/// what they say.
fn step_effects<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    step: &Step,
    who: Option<(u64, Option<(u32, u32)>)>,
    clock: Clock,
) {
    let me = s.players.get(&world).map_or(0, |p| p.character);
    for e in &step.effects {
        if crate::gives::special_effect(s, net, world, *e) {
            continue;
        }
        crate::effects::apply(
            s,
            net,
            world,
            *e,
            crate::effects::Cast {
                caster: me,
                spellcraft: 1.0,
                giver: who,
                ..crate::effects::Cast::default()
            },
            clock,
        );
    }
    if let Some(t) = step.text {
        progress::private(net, world, &crate::chat::show_text(2, progress::text(t)));
    }
}

/// The experience a quest's completion pays a player of a level: the quest's, weighed by its
/// challenge against the player's level as a kill's is by the creature's (a quest without a
/// challenge is not weighed); 2017 paid exactly this for 89 of 99 completions where the player's
/// level was known.
#[must_use]
pub fn completion_xp(xp: u64, challenge: i32, level: i32) -> u64 {
    if challenge == NO_CHALLENGE {
        return xp;
    }
    let share = crate::xp::kill_share(challenge - level).cast_unsigned();
    xp * share / 100
}

/// Make a quest update and tell the player.
pub fn apply<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    quest: u32,
    up: Update,
    xp: u64,
    clock: Clock,
) {
    let Some(def) = s.quests.quest(quest).cloned() else {
        s.unhandled.note(format!(
            "a step of quest {quest:08X}, which has no definition"
        ));
        return;
    };
    // The definition's effects for this update come first (2017: a recipe taught before the
    // quest given is told, a reward quest given before the completion that gives it is told).
    let effects_of = match up {
        Update::Bestow => update::BESTOW,
        Update::Advance(n) => update::ADVANCE | n,
        Update::Complete => update::COMPLETE,
    };
    // A subquest of a quest the player has at the phase it belongs to: its move is told as that
    // quest's.
    let parent = parent_held(s, world, quest);
    update_effects(s, net, world, &def, effects_of, clock);
    if xp > 0 {
        progress::award(s, net, world, xp, Award::Step);
    }
    if up == Update::Complete && def.xp > 0 {
        let level = s
            .players
            .get(&world)
            .map_or(1, |p| crate::examine::level_of(&p.qualities));
        let paid = completion_xp(def.xp, def.challenge, level);
        progress::award(s, net, world, paid, Award::Quest(def.name));
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let i = match p.play.quests.iter().position(|q| q.quest == quest) {
        Some(i) => i,
        None => {
            p.play.quests.push(QuestState {
                quest,
                status: status::UNDERWAY,
                phase: 0,
                started: clock.unix,
                updated: clock.unix,
            });
            p.play.quests.len() - 1
        }
    };
    let st = &mut p.play.quests[i];
    let kind = match up {
        Update::Bestow => {
            *st = QuestState {
                quest,
                status: status::UNDERWAY,
                phase: 1,
                started: clock.unix,
                updated: clock.unix,
            };
            update::BESTOW
        }
        Update::Advance(n) => {
            st.phase = n;
            st.updated = clock.unix;
            update::ADVANCE | n
        }
        Update::Complete => {
            st.status = status::COMPLETED;
            st.phase = 0;
            st.updated = clock.unix;
            update::COMPLETE
        }
    };
    let started = st.started;
    let phase = st.phase;
    // A phase done by subquests gives them with it, under way at their first phase.
    if up != Update::Complete {
        give_subquests(s, world, quest, phase, clock);
    }
    if !is_hidden(&s.quests, &def) {
        tell_record(s, net, world, &def, kind, clock);
    }
    crate::world::keep_player(s, world);
    if up == Update::Bestow {
        runs_out_after(s, world, &def, started, clock);
    }
    if let Some((parent, at)) = parent {
        subquest_moved(s, net, world, parent, at, clock);
    }
    if up == Update::Complete {
        triggered(s, net, world, Trigger::Done(quest), None, clock);
    }
}

/// Tell a player its record of a quest, as an update of a kind.
fn tell_record<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    def: &QuestDef,
    kind: u32,
    clock: Clock,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let Some(st) = p.play.quests.iter().find(|q| q.quest == def.id) else {
        return;
    };
    let game_now = net.game_time(clock.local);
    #[allow(clippy::cast_precision_loss)]
    let game_epoch = game_now - clock.unix as f64;
    let rec = record_in(&s.quests, &p.play.quests, def, st, game_now, game_epoch);
    progress::private(net, world, &update_call(rec, kind));
}

/// The quest, and its phase, that a quest is a subquest of for a player: one the player has under
/// way at a phase that lists it.
fn parent_held<S: Store>(s: &Server<S>, world: ConnId, sub: u32) -> Option<(u32, u32)> {
    let held = &s.players.get(&world)?.play.quests;
    s.quests
        .parents(sub)
        .find(|(q, at)| phase_of(held, *q) == *at)
}

/// Give a player the subquests of a quest's phase, each under way at its first phase (afresh if it
/// had them before), untold.
fn give_subquests<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    quest: u32,
    phase: u32,
    clock: Clock,
) {
    let Some(subs) = s.quests.subquests(quest, phase).map(|p| p.quests.clone()) else {
        return;
    };
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    for sub in subs {
        let fresh = QuestState {
            quest: sub,
            status: status::UNDERWAY,
            phase: 1,
            started: clock.unix,
            updated: clock.unix,
        };
        match p.play.quests.iter_mut().find(|q| q.quest == sub) {
            Some(q) => *q = fresh,
            None => p.play.quests.push(fresh),
        }
    }
}

/// A subquest of a quest's phase has moved: once all of the phase's subquests are done (one,
/// where one is enough) the quest moves on to its next phase; until then its record is told again,
/// its phase kept (2017: the third doctrine recorded moved the quest to its second phase, the first
/// two told with the phase kept, 9 of 9).
fn subquest_moved<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    quest: u32,
    phase: u32,
    clock: Clock,
) {
    let (Some(def), Some(subs)) = (
        s.quests.quest(quest).cloned(),
        s.quests.subquests(quest, phase).cloned(),
    ) else {
        return;
    };
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let mut done = subs.quests.iter().map(|q| done(&p.play.quests, *q));
    let through = if subs.any {
        done.any(|d| d)
    } else {
        done.all(|d| d)
    };
    let next = usize::try_from(phase).unwrap_or(usize::MAX) < def.journals.len();
    if through && next {
        apply(s, net, world, quest, Update::Advance(phase + 1), 0, clock);
        return;
    }
    if let Some(st) = p.play.quests.iter_mut().find(|q| q.quest == quest) {
        st.updated = clock.unix;
    }
    if !is_hidden(&s.quests, &def) {
        tell_record(s, net, world, &def, update::ADVANCE, clock);
    }
    crate::world::keep_player(s, world);
}

/// A player dies: its quests under way that fail when their player dies fail (2017: the Mist
/// Opal's, 2 of 2, in the death's tick before the death's message).
pub fn died<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let failing: Vec<u32> = s.players.get(&world).map_or_else(Vec::new, |p| {
        p.play
            .quests
            .iter()
            .filter(|q| q.status == status::UNDERWAY)
            .filter(|q| s.quests.quest(q.quest).is_some_and(|d| d.fails_on_death))
            .map(|q| q.quest)
            .collect()
    });
    for q in failing {
        fail(s, net, world, q, clock);
    }
}

/// Put on a player the effects a quest's definition names for an update of this kind, cast by the
/// player itself.
fn update_effects<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    def: &QuestDef,
    kind: u32,
    clock: Clock,
) {
    let me = s.players.get(&world).map_or(0, |p| p.character);
    for (_, e) in def.effects.iter().filter(|(k, _)| *k == kind) {
        crate::effects::apply(
            s,
            net,
            world,
            *e,
            crate::effects::Cast {
                caster: me,
                spellcraft: 1.0,
                ..crate::effects::Cast::default()
            },
            clock,
        );
    }
}

/// When a quest given at `started` runs out, if it has a duration: the timer that fails it.
fn runs_out_after<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    def: &QuestDef,
    started: i64,
    clock: Clock,
) {
    if def.duration <= 0.0 {
        return;
    }
    #[allow(clippy::cast_precision_loss)]
    let left = (started as f64 + def.duration - clock.unix as f64).max(0.0);
    s.after(
        clock,
        left,
        crate::Timer::QuestRunsOut {
            world,
            quest: def.id,
            started,
        },
    );
}

/// A player's quests under way with a duration, as it enters the world: the timer that fails each.
pub fn timers_at_entry<S: Store>(s: &mut Server<S>, world: ConnId, clock: Clock) {
    let held: Vec<QuestState> = s
        .players
        .get(&world)
        .map(|p| p.play.quests.clone())
        .unwrap_or_default();
    for st in held.iter().filter(|q| q.status == status::UNDERWAY) {
        if let Some(def) = s.quests.quest(st.quest).cloned() {
            runs_out_after(s, world, &def, st.started, clock);
        }
    }
}

/// The quests whose time ran out while their player was away fail, as of when it ran out (2017:
/// no entry listed a quest under way with no time left, 691 of 691 timed ones with time left).
pub fn ran_out_away(table: &yalain_content::QuestTable, held: &mut [QuestState], now_unix: i64) {
    for st in held.iter_mut().filter(|q| q.status == status::UNDERWAY) {
        let Some(def) = table.quest(st.quest) else {
            continue;
        };
        if def.duration <= 0.0 {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)]
        let ends = st.started + def.duration.ceil() as i64;
        if ends <= now_unix {
            st.status = status::FAILED;
            st.phase = 0;
            st.updated = ends;
        }
    }
}

/// A quest's time runs out: the quest given at `started`, still under way, fails (2017: 22 of 24
/// times a timed quest ran out while its player was in the world, within 2.4 s; the other two
/// were not seen again).
pub fn runs_out<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    quest: u32,
    started: i64,
    clock: Clock,
) {
    let under_way = s.players.get(&world).is_some_and(|p| {
        p.play
            .quests
            .iter()
            .any(|q| q.quest == quest && q.status == status::UNDERWAY && q.started == started)
    });
    if under_way {
        fail(s, net, world, quest, clock);
    }
}

/// A quest under way fails: the definition's effects for a failure, then the quest no longer under
/// way, told with the update of the fail kind (its record with the whole wait to be taken again)
/// unless the quest is hidden.
pub fn fail<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    quest: u32,
    clock: Clock,
) {
    let Some(def) = s.quests.quest(quest).cloned() else {
        return;
    };
    let under_way = s.players.get(&world).is_some_and(|p| {
        p.play
            .quests
            .iter()
            .any(|q| q.quest == quest && q.status == status::UNDERWAY)
    });
    if !under_way {
        return;
    }
    update_effects(s, net, world, &def, update::FAIL, clock);
    let game_now = net.game_time(clock.local);
    #[allow(clippy::cast_precision_loss)]
    let game_epoch = game_now - clock.unix as f64;
    let Some(st) = s.players.get_mut(&world).and_then(|p| {
        p.play
            .quests
            .iter_mut()
            .find(|q| q.quest == quest && q.status == status::UNDERWAY)
    }) else {
        return;
    };
    st.status = status::FAILED;
    st.phase = 0;
    st.updated = clock.unix;
    let st = *st;
    if !is_hidden(&s.quests, &def) {
        let rec = record(&def, &st, game_now, game_epoch);
        progress::private(net, world, &update_call(rec, update::FAIL));
    }
    crate::world::keep_player(s, world);
}

/// Is a quest one the world keeps hidden?
#[must_use]
pub fn hidden<S: Store>(s: &Server<S>, quest: u32) -> bool {
    s.quests.quest(quest).is_some_and(|d| d.hidden)
}

/// A player cancels a quest under way: it is told the quest is cancelled (its record with no
/// phase, its description for the journal, when it was cancelled and the wait to take it again),
/// and the quest is no longer under way. A quest that may not be cancelled, or is not under way,
/// is not answered.
pub fn cancel<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[crate::calls::Value],
    clock: Clock,
) {
    let Some(crate::calls::Value::Int(quest)) = v.first() else {
        s.unhandled.note("a quest cancel that does not read");
        return;
    };
    let quest = *quest;
    let Some(def) = s.quests.quest(quest).cloned() else {
        s.unhandled
            .note(format!("a cancel of quest {quest:08X}, unknown"));
        return;
    };
    let game_now = net.game_time(clock.local);
    #[allow(clippy::cast_precision_loss)]
    let game_epoch = game_now - clock.unix as f64;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(st) = p
        .play
        .quests
        .iter_mut()
        .find(|q| q.quest == quest && q.status == status::UNDERWAY)
    else {
        return;
    };
    if !def.cancellable {
        return;
    }
    st.status = status::CANCELLED;
    st.phase = 0;
    st.updated = clock.unix;
    let st = *st;
    let rec = write_record(
        &def,
        &st,
        Some(progress::text(def.description)),
        game_now,
        game_epoch,
    );
    progress::private(net, world, &update_call(rec, update::CANCEL));
    crate::world::keep_player(s, world);
}
