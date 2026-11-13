//! Quests: a step of a quest is triggered by using a person, an object or a held item (or a held
//! item on an object), giving an item to a person or an object, arriving through a portal, taking an item, killing a creature or completing
//! another quest, when the
//! player's quest is at the phase the step needs and the other quests it requires are where it
//! requires them. A step that gives a quest needs it not under way, and not done unless its wait
//! to be taken again is over: a quest with no wait is given again at once, one its definition
//! says is done once never (2017: of the quests the install says are done once, 0 of 49 were given
//! again; 62 others were, 115 times). One event runs every step it
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
//! update puts them on the player before anything else of it is told. A recall among them takes
//! the player at once through portal space, as the recall's skill would, and the update is told as
//! the player comes out (2017: the Gauntlets' advances, 47 of 47); a recall with no place in the
//! world leaves the player where it is. A step or an order of uses that names a place of its own
//! takes the player there instead, and its update's recall stays put.
//!
//! **Time running out.** A quest with a duration fails when its time runs out: its update of the
//! fail kind tells it failed, with the whole of its wait to be taken again. A few quests' durations
//! run in one phase only, from when that phase comes (2017: 5 of 164 timed quests, 4 in their first
//! phase, 1 in its second): before and after it the journal shows no time and the quest cannot fail
//! of it. Its time runs only
//! while its player is in the world: it stands still while the player is away, however long, and
//! a person's wait after giving a quest runs the same way. A quest's wait to be taken again runs
//! on the calendar, in the world or not. 2017, between two entries of a character: every timed
//! quest's time left fell by the same amount, at most the time that passed (585 of 585), and by
//! the time played where all of it was recorded; 1,651 waits to be taken again fell by the time
//! that passed, of which 1,583 had a character away a minute or more.
//!
//! **Death, leaving and portals.** A quest may fail when its player dies, when its player leaves
//! the world, or when its player is taken through portal space; the quest's setup in the install
//! says which, and a world's row may say otherwise. A death fails it in the death's tick, before
//! the death's message (2017: the Mist Opal's, 2 of 2). Leaving fails it as a logging off goes into
//! its last portal space, with the effects that end at logout, or as a character whose client went
//! away goes out of the world; one the server never saw leave (it stopped) fails as the character
//! next enters, as of when it was last kept. Any trip through portal space
//! fails it once the player is in portal space: a portal, a door, a recall, a shard, a step's or an
//! item's or an effect's sending, an operator's; not the return of a dead player, which its death
//! has already settled (2017: a totem's quest and its companion failed in the tick the player went
//! into portal space at a town's portal, after its portal space's call and effect; of 1,981 times
//! some other quest was under way as its player went into portal space, 205 quests, none failed
//! then). Of the install's 584 setups, 5 fail their quest on its player's death, 2 on leaving the
//! world and 2 on portal space.
//!
//! **A fellowship's step.** A step may make its update for the player's fellows too: each fellow
//! in the world for whom it may run is given the same update, told it alone.
//!
//! **Companion quests.** A quest may go with another's holder (the fellows sworn to protect the one
//! who carries a quest's burden): as a player's quest that others go with is done, each of those
//! under way, the player's own and each of its fellows' in the world, is done after it, paying
//! its own; as it fails (its player's death, its time running out, a step) or is cancelled, they
//! fail after it. A fellow who has left the fellowship by then keeps its own as it is (2017: a
//! totem gave its player both its quest and its companion, and both failed in one tick at the
//! player's portal; the companions' setups spread them to the fellowship and fail them on none of
//! their own holder's deaths, and their journals fail them as the bearer dies).
//!
//! **Subquests.** A phase of a quest may be done by doing other quests: they are given with the
//! phase, hidden, and each of their moves is told as the quest's own record, its phase kept, its
//! journal the phase's entry with those of the subquests still open; once all of them (or one,
//! where one is enough) are done, the quest moves on to its next phase.
//!
//! **Waits, levels and deaths.** A quest that failed or was cancelled is given again only once its
//! wait is over, as a done one is; one with a least level is given only to a player of that level,
//! the person saying why when it is not.
//!
//! **Kills.** A creature's template names the effects its death has, and a quest effect moves its
//! quest for the killer (the one credited with the kill) and, where the quest says so, for the
//! killer's fellows. An effect that gives a quest gives it at its first phase if it is not under
//! way and its wait is over; one that increments moves it on a phase, the last phase's completing it
//! unless that phase waits for something else; one that advances to a phase does so from the phase
//! before; one that completes does so from the last phase; none of these moves a quest the player
//! does not have under way, nor does anything for a stranger to the killer. A fellow of the killer
//! is moved alike, a quest that is limited so only within ten levels of the killer and in range of
//! it, and is told nothing when nothing moved. The killer is told when a giving was refused: its
//! update of the giving kind names why, in a record that holds nothing. A quest's setup says who it
//! is for: under its least level the giving is refused so; for someone it is not for (heroes alone,
//! or a required quest not at the status named) nothing is given or said. Steps a kill triggers run
//! first, and a quest one of them moved is not moved again.
//!
//! 2017, of the 4,068 corpses players saw of such creatures: a killer's quest under way moved on
//! every increment that could move it (518 of 518), and nothing moved where it was not under way
//! (0 of 4,101), nor at a last phase that waits (0 of 37). An advance from the phase before moved
//! it (51 of 52), from any other phase not (1 of 108). A completion, from the last phase (8 of 8).
//! A killer free to take a quest was given it (46 of 46); one with it under way or waiting was
//! told the refusal (117 of 118). A fellow near the killer's level moved as the killer did (88 of
//! 88 at up to 8 levels apart), not from 15 levels apart (0 of 17), and was never told a refusal
//! (0 of 94). No recording holds a fellow farther off than sight, or a stranger with an increment
//! to move: those limits are the rule's, and the fellows' share range's; the level limit is
//! inferred. A quest with no setup and no recording to say otherwise stays the killer's.
//!
//! **Deaths nearby.** A step may be moved by a creature's death within some meters of the player,
//! whoever killed it (another creature, or nobody the player knows): it runs for every player in
//! the world, out of portal space and alive, that far from where the creature fell or nearer.
//! Inferred: the install's one such death (a strike's target, which players may not harm) moves
//! its quest by an area effect of 150 m on the players there, and players' guides say only those
//! inside saw it move.
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
use yalain_store::{Store, StoredPosition};

use crate::graph::Builder;
use crate::levels::LevelTable;
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
    use auberean_script::native_object::QuestInfo;
    pub const UNDERWAY: u32 = QuestInfo::UNDERWAY;
    pub const FAILED: u32 = QuestInfo::FAILED;
    pub const COMPLETED: u32 = QuestInfo::COMPLETED;
    pub const CANCELLED: u32 = QuestInfo::CANCELLED;
}
pub mod update {
    pub const BESTOW: u32 = 0x1000_0000;
    pub const ADVANCE: u32 = 0x2000_0000;
    pub const COMPLETE: u32 = 0x3000_0000;
    pub const FAIL: u32 = 0x4000_0000;
    /// The kind of effect that moves a quest on a phase; no update is told of this kind.
    pub const INCREMENT: u32 = 0x6000_0000;
    pub const CANCEL: u32 = 0x8000_0000;
    /// Takes the quest off the client's list, whose journal then says so. No recording holds one:
    /// the client's own handling says what it does.
    pub const REMOVE: u32 = 0x5000_0000;
}

/// The player's request to cancel a quest.
pub const CANCEL_QUEST: u32 = 0x8213_0027;

/// The wait a cancelled quest names before it may be taken again, in seconds, where its row names
/// none: five minutes (2017: one value per quest, 29 of 29 cancels, 16 quests of 27 at this).
pub const CANCEL_WAIT: f64 = 300.0;

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
        q.cancel_wait.unwrap_or(CANCEL_WAIT)
    } else {
        q.retry
    }
}

/// What a quest's time counts from (world seconds), if its time is running: under way, with a
/// duration, in the phase its duration runs in if it names one. The quest's giving, or the coming of
/// a later phase its duration runs in.
fn time_since(q: &QuestDef, st: &QuestState) -> Option<i64> {
    if st.status != status::UNDERWAY || q.duration <= 0.0 {
        return None;
    }
    match q.timed_phase {
        0 => Some(st.started),
        n if n != st.phase => None,
        1 => Some(st.started),
        _ => Some(st.updated),
    }
}

/// A quest's name as the client is sent it: its reference, or the literal text standing in for one
/// the install can't write.
#[must_use]
pub fn name_text(name: &Words) -> TextInfo {
    match name {
        Words::Ref(string, table) => progress::text((*string, *table)),
        Words::Literal(l) => TextInfo {
            has_literal: 1,
            literal: Some(l.encode_utf16().collect()),
            ..TextInfo::default()
        },
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
    b.extend_from_slice(&name_text(&q.name).pack());
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
    let since = time_since(q, st);
    w32(&mut b, u32::from(since.is_some()));
    f64w(
        &mut b,
        since.map_or(-1.0, |from| (q.duration - (game_now - at(from))).max(0.0)),
    );
    // The wait to take it again: what is left of it (the whole wait the moment it is done).
    let left = || (wait_after(q, st.status) - (game_now - at(st.updated))).max(0.0);
    f64w(&mut b, if underway { 0.0 } else { left() });
    w32(&mut b, u32::from(q.play_fx));
    w32(&mut b, u32::from(q.cancellable));
    b
}

/// Why a giving was refused, as its update names it.
pub mod refused {
    /// The quest is under way (a move no quest's status allows).
    pub const UNDER_WAY: u32 = 4001;
    /// The quest is done and is given only once.
    pub const DONE: u32 = 4004;
    /// Its wait to be taken again is not over.
    pub const WAITING: u32 = 4005;
    /// The player is under its least level.
    pub const LEVEL_TOO_LOW: u32 = 10009;
}

/// A refused giving's record: the definition's name, description, icon and challenge, no status or
/// phase, given at no time and last done at the clock's zero, nothing to wait for.
fn write_refused(q: &QuestDef, st: &QuestState, game_epoch: f64) -> Vec<u8> {
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
    b.extend_from_slice(&name_text(&q.name).pack());
    ti(&mut b, q.description);
    w32(&mut b, q.icon);
    w32(&mut b, st.status);
    w32(&mut b, st.phase);
    ti(&mut b, (0, 0));
    w32(&mut b, q.challenge.cast_unsigned());
    f64w(&mut b, 0.0);
    f64w(&mut b, game_epoch);
    w32(&mut b, 0);
    f64w(&mut b, -1.0);
    f64w(&mut b, 0.0);
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

/// The update that tells a giving was refused: why, a record of the quest holding nothing but what
/// its definition says (no status, no phase, no times: 2017, 148 of 148), the giving kind. The client
/// updates nothing for it.
#[must_use]
pub fn refusal_call(q: &QuestDef, why: u32, game_epoch: f64) -> auberean_protocol::ServerMessage {
    let st = QuestState {
        quest: q.id,
        status: 0,
        phase: 0,
        started: 0,
        updated: 0,
    };
    let mut b = Builder::new();
    b.native(
        QUEST_INFO,
        NativeObject::Packed {
            ty: QUEST_INFO.into(),
            bytes: write_refused(q, &st, game_epoch),
        },
    );
    session::call(
        UPDATE_QUEST,
        &[
            Arg::Int(why),
            Arg::Object(b.graph()),
            Arg::Int(update::BESTOW),
        ],
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
    teller_name: TextInfo,
    line: &Words,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let msg = crate::talk::text_for(line, &crate::player::world_name(&p.name, p.sex));
    tell_text(net, world, teller, teller_name, msg);
}

/// A person tells the player a text as it is.
pub fn tell_text(
    net: &mut ServerNet,
    world: ConnId,
    teller: u64,
    teller_name: TextInfo,
    msg: TextInfo,
) {
    let m = session::call(
        crate::chat::function::HEAR_TELL,
        &[
            Arg::Int(0),
            crate::chat::text_arg(msg),
            crate::chat::text_arg(teller_name),
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

/// Has a quest of a player's failed (a step failed it, or its time ran out), not given again since:
/// while `during` is under way, a failure of a giving that came after that quest's own (a timer
/// given in an earlier run of it does not count).
#[must_use]
pub fn failed_during(held: &[QuestState], quest: u32, during: Option<u32>) -> bool {
    let Some(f) = held
        .iter()
        .find(|q| q.quest == quest && q.status == status::FAILED)
    else {
        return false;
    };
    during
        .and_then(|d| {
            held.iter()
                .find(|q| q.quest == d && q.status == status::UNDERWAY)
        })
        .is_none_or(|own| f.started >= own.started)
}

/// May a step run for a player now: its quest at the phase it needs (to be given: not under way,
/// its wait to be taken again over, the player of its least level), the quests it requires where
/// it requires them, the craft skills it waits on at their levels, an untraining under way where it
/// waits on one, a hero where it is for heroes alone, and the items it takes held.
fn may_run<S: Store>(s: &Server<S>, world: ConnId, st: &Step, now_unix: i64) -> bool {
    may_run_at_any_level(s, world, st, now_unix)
        && level_allows(s, world, st)
        && crate::craft::step_allows(s, world, st)
        && crate::skills::untraining_allows_step(s, world, st)
        && crate::focus::allows_step(s, world, st)
}

/// May a step run for a player now (a step due after a wait runs only if it still may).
#[must_use]
pub fn may_run_now<S: Store>(s: &Server<S>, world: ConnId, st: &Step, now_unix: i64) -> bool {
    may_run(s, world, st, now_unix)
}

/// Is the player of the level a step that gives a quest needs (the quest's least level)?
fn level_allows<S: Store>(s: &Server<S>, world: ConnId, st: &Step) -> bool {
    let Some(level) = s
        .players
        .get(&world)
        .map(|p| crate::examine::level_of(&p.qualities))
    else {
        return false;
    };
    if st.levels.is_some_and(|(a, b)| level < a || level > b) {
        return false;
    }
    if st.needs != 0 || st.update != Update::Bestow {
        return true;
    }
    level >= s.quests.quest(st.quest).map_or(0, |d| d.min_level)
}

/// May a step run for a player now, its quest's least level and the craft skills it waits on
/// aside.
fn may_run_at_any_level<S: Store>(s: &Server<S>, world: ConnId, st: &Step, now_unix: i64) -> bool {
    let Some(p) = s.players.get(&world) else {
        return false;
    };
    let held = &p.play.quests;
    if phase_of(held, st.quest) != st.needs
        || !crate::events::step_runs(s, st)
        || !crate::tallies::step_for(s, st)
    {
        return false;
    }
    // A step whose gift can't be made doesn't run: it would take what it takes for nothing.
    let picked = s.item_rows.step_pick(st).unwrap_or_default();
    if !st
        .gives
        .iter()
        .chain(picked.iter().map(|(t, _)| t))
        .all(|t| crate::makes::can_make(s, *t))
    {
        return false;
    }
    if !st.races.is_empty() && !st.races.contains(&p.race) {
        return false;
    }
    if !s
        .kingdom_rows
        .step_for(st, crate::kingdoms::counted_of(&p.qualities))
    {
        return false;
    }
    if !s.story.step_for(st, &p.play.scenes) {
        return false;
    }
    if st.needs == 0 {
        let Some(def) = s.quests.quest(st.quest) else {
            return false;
        };
        if !given_again(def, held, now_unix) {
            return false;
        }
        if st.update == Update::Bestow && !setup_allows(&s.quests, def, p, now_unix) {
            return false;
        }
    }
    let requires = st.requires.iter().all(|(q, need)| match need {
        Need::Phase(n) => phase_of(held, *q) == *n,
        Need::Done => done(held, *q),
        Need::Free => {
            phase_of(held, *q) == 0
                && s.quests
                    .quest(*q)
                    .is_none_or(|d| given_again(d, held, now_unix))
        }
        Need::Failed => failed_during(held, *q, Some(st.quest)),
    });
    requires
        && st
            .takes
            .iter()
            .all(|t| crate::inventory::holds(s, world, *t))
}

/// May a quest be given to a player, by who its setup says it is for, the least level aside: a hero
/// for a quest for heroes alone, a player with the quest it requires at the status it names. In 2017
/// every giving of a hero's quest whose character's quests are known went to a hero (14 of 14) and
/// no one else's kill named one to refuse, so what heroes alone means for others is read from the setup. A kill naming a quest whose
/// required quest was never had gave nothing and said nothing (16 of 16, 7 of them the killer's).
fn setup_allows(
    table: &QuestTable,
    def: &QuestDef,
    p: &crate::world::Player,
    now_unix: i64,
) -> bool {
    !(def.hero_only && !crate::focus::is_hero(&p.qualities))
        && def.required.is_none_or(|(quest, needed)| {
            let retry = table.quest(quest).map_or(0.0, |q| q.retry);
            crate::item_use::quest_refusal(&p.play.quests, quest, needed, retry, now_unix).is_none()
        })
}

/// May a quest a player does not have under way be given it again: never had, or its wait over
/// (a done one's the quest's own, none meaning at once, unless the quest is done once; a failed
/// one's the same; a cancelled one's its cancel wait). 2017: a failed quest's shrine said only its
/// refusal while the wait ran, 2 of 2; quests whose completion named no wait were given again 23
/// times (9 of 31), never one done once.
pub(crate) fn given_again(def: &QuestDef, held: &[QuestState], now_unix: i64) -> bool {
    held.iter().find(|q| q.quest == def.id).is_none_or(|q| {
        #[allow(clippy::cast_precision_loss)]
        let waited = (now_unix - q.updated) as f64;
        let wait = wait_after(def, q.status);
        match q.status {
            status::COMPLETED => !def.once && waited >= wait,
            status::FAILED | status::CANCELLED => waited >= wait,
            _ => true,
        }
    })
}

/// The quest a trigger would give a player but for the player's level (it is under the quest's
/// least level), craft skills (under a level its step waits on) or not being a hero (its step is
/// for heroes alone), if any.
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
            s.quests.quest(st.quest).is_some_and(|d| {
                (level < d.min_level
                    || !crate::craft::step_allows(s, world, st)
                    || !crate::focus::allows_step(s, world, st))
                    && phase_of(&p.play.quests, d.id) == 0
            }) && may_run_at_any_level(s, world, st, now_unix)
        })
        .map(|st| st.quest)
}

/// Is a quest one its player is never told of: hidden, or a subquest of another.
#[must_use]
pub fn is_hidden(table: &QuestTable, q: &QuestDef) -> bool {
    q.hidden || table.parents(q.id).next().is_some()
}

/// Something happened that may move quests on: run every step it triggers (each quest at most
/// once; a use, a quest under way first and one step of the server's errands). Returns whether one
/// ran. `who` is the person or object used (for its line), with its name.
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

/// Something happened that may move quests on: run every step it triggers (each quest at most
/// once). Returns the quests they moved.
fn moved_by<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    trigger: Trigger,
    clock: Clock,
) -> Vec<u32> {
    let mut moved = Vec::new();
    run_triggered(s, net, world, trigger, None, None, clock, &mut moved);
    moved
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
    given: Option<crate::gives::Given>,
    clock: Clock,
) -> (bool, bool) {
    let mut moved = Vec::new();
    let kept = run_triggered(s, net, world, trigger, who, given, clock, &mut moved);
    (!moved.is_empty(), kept)
}

/// Run every step a trigger triggers, listing the quests moved; returns whether a step kept the
/// item given.
#[allow(clippy::too_many_arguments)]
fn run_triggered<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    trigger: Trigger,
    who: Option<(u64, Option<(u32, u32)>)>,
    mut given: Option<crate::gives::Given>,
    clock: Clock,
    moved: &mut Vec<u32>,
) -> bool {
    let mut kept = false;
    // A step wanting a count runs for that many given and no other.
    let handed = given.map(|g| g.quantity);
    // A person who gives several quests gives one of them at a time.
    let several = match trigger {
        Trigger::Use(_) => givings(s, trigger),
        _ => Vec::new(),
    };
    // A step that leads somewhere is run by a use of a person or an object, a give to one, a kill
    // or a take; an item's use on one is the trip's first, its update on arrival. A take's trip
    // flashes on the item taken.
    let by = who.map(|(id, _)| id).or_else(|| match trigger {
        Trigger::Take(t) => s.players.get(&world).and_then(|p| {
            p.play
                .items
                .iter()
                .rev()
                .find(|h| h.template == t)
                .map(|h| h.id)
        }),
        _ => None,
    });
    let leads_on = matches!(
        trigger,
        Trigger::Use(_)
            | Trigger::Give(..)
            | Trigger::GiveClass(..)
            | Trigger::Take(_)
            | Trigger::Kill(_)
            | Trigger::KillIn(..)
            | Trigger::KillNear(..)
    ) && by.is_some();
    let used = matches!(trigger, Trigger::Use(_));
    // A use runs one step of the server's errands: the person's next comes at the next use.
    let mut errand_ran = false;
    // A step whose chance did not come up stays out of the rest of this trigger's steps.
    let mut missed: Vec<Step> = Vec::new();
    loop {
        // Reckoned at each step: one given (now under way) blocks the rest.
        let blocked = several.len() >= 2 && person_blocks(s, world, &several, clock.unix);
        let seen = moved.clone();
        let errand_done = used && errand_ran;
        let runs = |s: &Server<S>, st: &Step| {
            st.trigger == trigger
                && (st.to.is_none() || leads_on)
                && handed.is_none_or(|n| takes_count(st, n))
                && !seen.contains(&st.quest)
                && !(blocked && st.needs == 0 && several.contains(&st.quest))
                && !(errand_done && errand(st.quest))
                && !missed.contains(st)
                && may_run(s, world, st, clock.unix)
        };
        // A use moves on a quest under way before it gives one.
        let under_way = |s: &Server<S>, st: &Step| used && st.needs != 0 && runs(s, st);
        let found = s
            .quests
            .steps
            .iter()
            .find(|st| under_way(s, st))
            .or_else(|| s.quests.steps.iter().find(|st| runs(s, st)));
        let Some(mut step) = found.cloned() else {
            return kept;
        };
        if several.len() >= 2 && step.needs == 0 && several.contains(&step.quest) {
            // Which of them: one at random of those it may give.
            let may: Vec<Step> = s
                .quests
                .steps
                .iter()
                .filter(|st| st.needs == 0 && several.contains(&st.quest) && runs(s, st))
                .cloned()
                .collect();
            let i = s.rng.below(may.len());
            step = may[i].clone();
        }
        // A step with a chance runs only when it comes up.
        if step.chance.is_some_and(|c| !s.rng.chance(c)) {
            missed.push(step);
            continue;
        }
        moved.push(step.quest);
        errand_ran |= errand(step.quest);
        let trip = s.players.get(&world).and_then(|p| p.trip.as_ref());
        if trip.is_some_and(|t| t.step.as_ref().is_some_and(|c| c.quest == step.quest)) {
            // Its trip carries this quest's move: that comes on landing, and only that.
            continue;
        }
        let free = trip.is_none();
        let sent = step.to.zip(by);
        let kill = matches!(
            trigger,
            Trigger::Kill(_) | Trigger::KillIn(..) | Trigger::KillNear(..)
        );
        if let (true, true, Some((place, by))) = (free, kill, sent) {
            // A kill's step goes with the trip, its update told on landing (2017: 47 of 47 Gauntlet
            // kills, in portal space's exit's tick; the trip within 0.1 s of the death, 42 of 42
            // whose death was seen).
            let carries = step.to_fellows;
            let room = room_for(s, place);
            crate::places::sends_on(s, world, by, room, Some(step), clock);
            if carries {
                fellows_go(s, world, by, place, clock);
            }
            continue;
        }
        let received = run_with(s, net, world, &step, who, given, clock);
        if let Some((place, by)) = sent {
            // One already on its way goes nowhere, whichever room.
            if s.players.get(&world).is_some_and(|p| p.trip.is_none()) {
                let room = room_for(s, place);
                crate::places::sends_on(s, world, by, room, None, clock);
            }
            if step.to_fellows {
                fellows_go(s, world, by, place, clock);
            }
        }
        // The item given is kept by the first step that takes it.
        if given.is_some_and(|g| step.takes.contains(&g.template)) {
            given = None;
            kept = true;
        }
        for template in received {
            took(s, net, world, template, clock);
        }
    }
}

/// Is a quest one of the server's own errands (an id of the `0x8000_0000` range, which no package of
/// the install sets up)? A use of a person runs one step of them: in 2017, a use that completed one
/// gave the person's next at the player's next use, 13 of 13, never in the same use; one that
/// completed a quest the install sets up gave the next in the same use, 45 of 45.
fn errand(quest: u32) -> bool {
    quest & 0xF000_0000 == 0x8000_0000
}

/// How far from where it lands a player may be and still be in that room, in meters.
const ROOM_REACH: f32 = 80.0;

/// The room of a row of like rooms a player is sent to, where a step leads to one of them: at random
/// among those with no player and a creature standing, else with no player, else any (2017's
/// landings fell across 3 to 6 rooms a row; how they were chosen isn't recorded, so the order is
/// ours). A player is in the room whose landing is nearest it, within reach. A place in no row is
/// that place.
pub(crate) fn room_for<S: Store>(
    s: &mut Server<S>,
    place: yalain_content::quests::Place,
) -> yalain_content::quests::Place {
    let Some(row) = s
        .quests
        .rooms
        .iter()
        .find(|row| row.iter().any(|r| r.place == place))
    else {
        return place;
    };
    let at = |p: yalain_content::quests::Place| StoredPosition {
        cell: p.cell,
        origin: p.origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let taken: Vec<bool> = {
        let mut taken = vec![false; row.len()];
        for p in s.players.values() {
            let nearest = row
                .iter()
                .enumerate()
                .filter_map(|(i, r)| {
                    crate::presence::distance(&at(r.place), &p.position)
                        .filter(|d| *d <= ROOM_REACH)
                        .map(|d| (i, d))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((i, _)) = nearest {
                taken[i] = true;
            }
        }
        taken
    };
    let mut standing = vec![false; row.len()];
    for t in s.things.by_id.values() {
        if t.creature.is_none() || !t.alive() {
            continue;
        }
        let at = t.at.map(|a| a.cell);
        if let Some(i) = row.iter().position(|r| Some(r.holds) == at) {
            standing[i] = true;
        }
    }
    let rank = |i: usize| match (taken[i], standing[i]) {
        (false, true) => 2,
        (false, false) => 1,
        (true, _) => 0,
    };
    let best = (0..row.len()).map(rank).max().unwrap_or(0);
    let among: Vec<yalain_content::quests::Place> = (0..row.len())
        .filter(|i| rank(*i) == best)
        .map(|i| row[i].place)
        .collect();
    let i = s.rng.below(among.len());
    among[i]
}

/// A step that takes its player somewhere takes the player's fellowship along: each fellow in the
/// world goes there too, through portal space, wherever it is, unless it is dead or already on its
/// way somewhere. Where the place is one of a row of like rooms, each fellow is sent to a room of
/// its own. A fellow is only sent: what the step does to a quest reaches it only as the step says.
pub(crate) fn fellows_go<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    by: u64,
    place: yalain_content::quests::Place,
    clock: Clock,
) {
    let going: Vec<ConnId> = crate::fellowship::fellows_in_world(s, world)
        .into_iter()
        .filter(|w| {
            s.players
                .get(w)
                .is_some_and(|p| p.scene_drawn && !crate::combat::is_dead(p))
        })
        .collect();
    // One already on its way somewhere keeps going there.
    for w in going {
        let room = room_for(s, place);
        crate::places::sends_on(s, w, by, room, None, clock);
    }
}

/// What a quest effect does to its quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Gives it.
    Bestow,
    /// Moves it on a phase, completing it from its last.
    Increment,
    /// Moves it on to this phase from the phase before.
    Advance(u32),
    /// Completes it from its last phase.
    Complete,
    /// Fails it.
    Fail,
}

impl Move {
    /// The move a quest effect's update makes: a giving, a completion, a failure, a phase more, or
    /// a move to the phase it names.
    #[must_use]
    pub const fn of_update(update: u32) -> Self {
        match update & 0xF000_0000 {
            update::BESTOW => Self::Bestow,
            update::COMPLETE => Self::Complete,
            update::FAIL => Self::Fail,
            update::INCREMENT => Self::Increment,
            _ => Self::Advance(update & 0x0FFF_FFFF),
        }
    }
}

/// An effect on a quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestEffect {
    pub quest: u32,
    pub act: Move,
}

/// What an effect's name says it does to a quest: the kind of effect, then the quest's name in the
/// enumeration of quests, and for an advance the phase (`quest_id` answers the enumeration; every
/// name of the 273 the install holds a saved state for says what that state says).
#[must_use]
pub fn effect_named(name: &str, quest_id: impl Fn(&str) -> Option<u32>) -> Option<QuestEffect> {
    let head = name.get(..4)?;
    if !head.eq_ignore_ascii_case("Eff_") {
        return None;
    }
    let rest = &name[4..];
    let (verb, rest) = [
        ("Increment", Move::Increment),
        ("Bestow", Move::Bestow),
        ("Complete", Move::Complete),
        ("Advance", Move::Advance(0)),
        ("Fail", Move::Fail),
    ]
    .into_iter()
    .find_map(|(w, v)| Some((v, rest.strip_prefix(w)?)))?;
    let rest = rest.strip_prefix("Quest")?;
    let id = |r: &str| quest_id(&format!("Quest{r}"));
    // A quest's own name may end in a phase's word.
    let (base, phase) = rest
        .rfind("Phase")
        .and_then(|i| Some((&rest[..i], rest[i + 5..].parse::<u32>().ok()?)))
        .map_or((rest, None), |(b, p)| (b, Some(p)));
    match (verb, phase) {
        (Move::Advance(_), Some(p)) => Some(QuestEffect {
            quest: id(base)?,
            act: Move::Advance(p),
        }),
        (Move::Advance(_), None) => None,
        (act, _) => Some(QuestEffect {
            quest: id(rest).or_else(|| id(base))?,
            act,
        }),
    }
}

/// How many acquiring effects a template may name.
const ACQUIRE_EFFECTS: u32 = 3;

/// Does a template name an effect on being acquired that isn't a quest's? The world does none of
/// those.
#[must_use]
pub fn acquiring_does_more_than_quests(props: &yalain_content::Props) -> bool {
    (1..=ACQUIRE_EFFECTS)
        .filter_map(|n| props.text(&format!("Item_AcquireEffect{n}")))
        .any(|n| effect_named(n, |_| Some(0)).is_none())
}

/// An item came into a player's hands: the steps its taking triggers run, then the quest effects
/// its template names on being acquired on each quest no step moved (2017: 38 of 54 acquirings
/// moved their quest in the same second; 14 of the rest were past that phase, 2 a hidden quest's),
/// then on the taker's fellows the effects whose quests reach them, as a kill's do (2017: an item
/// picked up, recorded from both sides, moved the fellow's quest in the same moment, the fellow
/// doing nothing and standing 1.6 m off; 34 levels apart, its quest limiting neither). Nothing is
/// said of a quest the effect could not move.
pub fn took<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    template: u32,
    clock: Clock,
) {
    // A glyph's tip comes first, as the item is taken (2017: 2 of 2, before the quest it moved).
    crate::popups::took(s, net, world, template);
    let moved = moved_by(s, net, world, Trigger::Take(template), clock);
    let named: Vec<QuestEffect> = s.catalog.get(template).map_or_else(Vec::new, |t| {
        (1..=ACQUIRE_EFFECTS)
            .filter_map(|n| t.props.text(&format!("Item_AcquireEffect{n}")))
            .filter_map(|n| effect_named(n, |q| s.quest_ids.get(q).copied()))
            .collect()
    });
    for e in named.iter().filter(|e| !moved.contains(&e.quest)) {
        moves(s, net, world, *e, false, clock);
    }
    if named.is_empty() {
        return;
    }
    // Every effect reaches the fellows, a quest a step moved too: a step that moved its fellows
    // already left them past the phase a giving, an advance or a completion moves from. An
    // increment would move them twice, so one a step moved stays the taker's.
    let fellows = crate::fellowship::fellows_in_world(s, world);
    for w in fellows {
        for e in &named {
            let twice = e.act == Move::Increment && moved.contains(&e.quest);
            if !twice && reaches(s, *e, world, w) {
                moves(s, net, w, *e, false, clock);
            }
        }
    }
}

/// How far apart in level a fellow and a killer may be for a move limited by level to reach the
/// fellow. Inferred: 2017's fellows moved at up to 8 levels apart, 88 of 88, and not from 15, 0 of
/// 17, and nothing was recorded between them.
pub const KILL_LEVELS: i32 = 10;

/// A creature died with its kill credited to a player: the steps the kill triggers run for the
/// killer (those of its template, then those of the one camp that made it, each quest moved at most
/// once), then the template's quest effects on each quest no step moved, then on the killer's
/// fellows the effects whose quests reach them. Each effect names with its stat the range of its
/// quest's phases it moves.
pub fn killed<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    killer: ConnId,
    (creature, template, camp): (u64, u32, Option<usize>),
    effects: &[(String, u32)],
    clock: Clock,
) {
    let mut moved = Vec::new();
    let who = Some((creature, None));
    let trigger = Trigger::Kill(template);
    run_triggered(s, net, killer, trigger, who, None, clock, &mut moved);
    if let Some(camp) = camp.and_then(|c| u32::try_from(c).ok()) {
        let trigger = Trigger::KillIn(template, camp);
        run_triggered(s, net, killer, trigger, who, None, clock, &mut moved);
    }
    let named: Vec<(QuestEffect, u32)> = effects
        .iter()
        .filter_map(|(n, range)| Some((effect_named(n, |q| s.quest_ids.get(q).copied())?, *range)))
        .collect();
    if named.is_empty() {
        return;
    }
    for (e, range) in named.iter().filter(|(e, _)| !moved.contains(&e.quest)) {
        moves_by_kill(s, net, killer, *e, *range, true, clock);
    }
    let fellows = crate::fellowship::fellows_in_world(s, killer);
    for w in fellows {
        for (e, range) in &named {
            if reaches(s, *e, killer, w) {
                moves_by_kill(s, net, w, *e, *range, false, clock);
            }
        }
    }
}

/// A creature died: the steps a death of its template within some meters triggers run for each
/// player that far from where it died or nearer, whoever killed it: in the world, out of portal
/// space and alive.
pub fn died_near<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    (creature, template): (u64, u32),
    at: &StoredPosition,
    clock: Clock,
) {
    let mut reaches: Vec<u32> = s
        .quests
        .steps
        .iter()
        .filter_map(|st| match st.trigger {
            Trigger::KillNear(t, m) if t == template => Some(m),
            _ => None,
        })
        .collect();
    reaches.sort_unstable();
    reaches.dedup();
    for meters in reaches {
        #[allow(clippy::cast_precision_loss)]
        let within = meters as f32;
        let near: Vec<ConnId> = s
            .players
            .iter()
            .filter(|(_, p)| p.scene_drawn && p.trip.is_none() && !crate::combat::is_dead(p))
            .filter(|(_, p)| {
                crate::presence::distance(at, &p.position).is_some_and(|d| d <= within)
            })
            .map(|(w, _)| *w)
            .collect();
        let trigger = Trigger::KillNear(template, meters);
        for world in near {
            let mut moved = Vec::new();
            let who = Some((creature, None));
            run_triggered(s, net, world, trigger, who, None, clock, &mut moved);
        }
    }
}

/// What a template's properties name for its use by what it does to a quest, by name.
#[must_use]
pub fn named_for_use<S: Store>(s: &Server<S>, props: &yalain_content::Props) -> Vec<QuestEffect> {
    (1..=8)
        .filter_map(|i| props.text(&format!("{}{i}", crate::item_use::USED.0)))
        .filter_map(|n| effect_named(n, |q| s.quest_ids.get(q).copied()))
        .collect()
}

/// Does an effect a killer's kill (or a taker's take) has on a quest reach this fellow: the quest
/// shares, and the fellow is within the limits it names?
fn reaches<S: Store>(s: &Server<S>, e: QuestEffect, by: ConnId, fellow: ConnId) -> bool {
    let Some(f) = s.quests.quest(e.quest).map(|d| d.fellows) else {
        return false;
    };
    let (Some(a), Some(b)) = (s.players.get(&by), s.players.get(&fellow)) else {
        return false;
    };
    let levels = crate::examine::level_of(&a.qualities) - crate::examine::level_of(&b.qualities);
    f.shared
        && (!f.near_level || levels.abs() <= KILL_LEVELS)
        && (!f.near_by
            || crate::presence::distance(&a.position, &b.position)
                .is_some_and(|d| d <= crate::fellowship::SHARE_RANGE))
}

/// A quest effect on a player: its quest moves if its status allows it; for one a giving is refused
/// to (`told`: the one it was put on), its refusal told.
pub fn moves<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    e: QuestEffect,
    told: bool,
    clock: Clock,
) {
    let decision = decide(s, world, e, None, clock);
    carry_out(s, net, world, e, decision, told, clock);
}

/// A kill's quest effect on a player, as [`moves`] but for its increment: that moves only a phase
/// of the range its creature's death names (`range`, the effect's stat), and from the last phase of
/// that range ends the quest. A phase of another range is counted by other kills, or waits for
/// something other than a kill.
fn moves_by_kill<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    e: QuestEffect,
    range: u32,
    told: bool,
    clock: Clock,
) {
    let decision = decide(s, world, e, Some(range), clock);
    carry_out(s, net, world, e, decision, told, clock);
}

fn carry_out<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    e: QuestEffect,
    decision: Decision,
    told: bool,
    clock: Clock,
) {
    match decision {
        Decision::Moves(up) => apply(s, net, world, e.quest, up, 0, clock),
        Decision::Fails => fail(s, net, world, e.quest, clock),
        Decision::Refused(def, why) => refuse(s, net, world, &def, why, told, clock),
        Decision::Nothing => {}
    }
}

/// Would a quest effect move its quest for a player now (or fail it)? As [`moves`] judges it,
/// changing nothing.
#[must_use]
pub fn would_move<S: Store>(s: &Server<S>, world: ConnId, e: QuestEffect, clock: Clock) -> bool {
    matches!(
        decide(s, world, e, None, clock),
        Decision::Moves(_) | Decision::Fails
    )
}

/// What a quest effect does to a player's quest now.
enum Decision {
    Moves(Update),
    Fails,
    /// A giving refused: the quest, and why.
    Refused(Box<QuestDef>, u32),
    Nothing,
}

/// What a quest effect does to a player's quest now; an increment of a kill's moves only a phase of
/// its `range`.
fn decide<S: Store>(
    s: &Server<S>,
    world: ConnId,
    e: QuestEffect,
    range: Option<u32>,
    clock: Clock,
) -> Decision {
    let Some(def) = s.quests.quest(e.quest) else {
        return Decision::Nothing;
    };
    let Some(p) = s.players.get(&world) else {
        return Decision::Nothing;
    };
    let phase = phase_of(&p.play.quests, e.quest);
    let last = u32::try_from(def.journals.len()).unwrap_or(0);
    let no = |why| Decision::Refused(Box::new(def.clone()), why);
    let up = match e.act {
        Move::Bestow if phase != 0 => return no(refused::UNDER_WAY),
        Move::Bestow => {
            let held = &p.play.quests;
            if given_again(def, held, clock.unix) {
                // Under its least level it is refused as the other reasons are (2017: 2 of 2, the
                // givings at or above it 83 of 83, 9 more of unknown level); by who it is for,
                // nothing is said.
                if crate::examine::level_of(&p.qualities) < def.min_level {
                    return no(refused::LEVEL_TOO_LOW);
                }
                if !setup_allows(&s.quests, def, p, clock.unix) {
                    return Decision::Nothing;
                }
                Update::Bestow
            } else {
                let why = if done(held, e.quest) && def.once {
                    refused::DONE
                } else {
                    refused::WAITING
                };
                return no(why);
            }
        }
        _ if phase == 0 => return Decision::Nothing,
        Move::Increment if range.is_some_and(|r| def.range(phase) != r) => {
            return Decision::Nothing;
        }
        Move::Increment if phase < last => Update::Advance(phase + 1),
        Move::Increment if range.is_none() && def.last_waits() => return Decision::Nothing,
        Move::Increment => Update::Complete,
        Move::Advance(n) if n == phase + 1 => Update::Advance(n),
        Move::Advance(_) => return Decision::Nothing,
        Move::Complete if phase >= last => Update::Complete,
        Move::Complete => return Decision::Nothing,
        Move::Fail => return Decision::Fails,
    };
    Decision::Moves(up)
}

/// Tell a player its giving of a quest was refused, unless the quest is hidden or it is not the one
/// the effect was put on.
fn refuse<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    def: &QuestDef,
    why: u32,
    told: bool,
    clock: Clock,
) {
    if !told || is_hidden(&s.quests, def) {
        return;
    }
    let game_now = net.game_time(clock.local);
    #[allow(clippy::cast_precision_loss)]
    let game_epoch = game_now - clock.unix as f64;
    progress::private(net, world, &refusal_call(def, why, game_epoch));
}

/// How long a person who gives several quests with no wait of their own gives none after giving
/// one, in seconds of the player's time in the world (2017, from the last one given: Stubby gave
/// none at 371 to 6,976 s, 18 of 18 uses, and one at 7,257 s and later, 18 of 18; Lupe 12 of 13;
/// 6 uses said none with less than this played though more than this had passed). It is those
/// quests' own duration.
pub const PERSON_WAIT: f64 = 7200.0;

/// The quests a trigger gives (its steps that give a quest), each once, in step order.
fn givings<S: Store>(s: &Server<S>, trigger: Trigger) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for st in &s.quests.steps {
        if st.trigger == trigger
            && st.needs == 0
            && st.update == Update::Bestow
            && !out.contains(&st.quest)
        {
            out.push(st.quest);
        }
    }
    out
}

/// Do a person's quests follow one another, each given once the one before it is done? (2017: two
/// characters did one person's 33 in one order, with 19 and 17 of the 32 gaps under [`PERSON_WAIT`];
/// the next was given 3.7 s after the last was done, 1 of 1.)
fn in_a_chain<S: Store>(s: &Server<S>, quests: &[u32]) -> bool {
    s.quests.steps.iter().any(|st| {
        st.needs == 0
            && st.update == Update::Bestow
            && quests.contains(&st.quest)
            && st
                .requires
                .iter()
                .any(|(q, need)| *need == Need::Done && quests.contains(q))
    })
}

/// Has a person who gives several quests, none of them with a wait of its own and none under way
/// for the player and not in a chain, given the player one of them less than [`PERSON_WAIT`] ago?
fn person_waits<S: Store>(s: &Server<S>, world: ConnId, quests: &[u32], now_unix: i64) -> bool {
    let Some(p) = s.players.get(&world) else {
        return false;
    };
    if in_a_chain(s, quests) {
        return false;
    }
    let no_wait = quests
        .iter()
        .all(|q| s.quests.quest(*q).is_some_and(|d| d.retry <= 0.0));
    no_wait
        && quests.iter().all(|q| phase_of(&p.play.quests, *q) == 0)
        && p.play.quests.iter().any(|h| {
            #[allow(clippy::cast_precision_loss)]
            let since = (now_unix - h.started) as f64;
            quests.contains(&h.quest) && since < PERSON_WAIT
        })
}

/// Does a person who gives several quests give none of them now: one is under way, or they have no
/// wait of their own and it gave one less than [`PERSON_WAIT`] ago (2017: Stubby, Kanten and Lupe
/// gave none while one of theirs was under way, 31 of 32 uses, and never two at one use, 71 of 71;
/// the starting towns' people, whose quests have waits, gave the next as a use completed one, 13 of
/// 13).
fn person_blocks<S: Store>(s: &Server<S>, world: ConnId, quests: &[u32], now_unix: i64) -> bool {
    let Some(p) = s.players.get(&world) else {
        return true;
    };
    quests.iter().any(|q| phase_of(&p.play.quests, *q) != 0)
        || person_waits(s, world, quests, now_unix)
}

/// Would a trigger give a quest but for its person having given one too lately (it says its
/// too-soon line instead)?
#[must_use]
pub fn too_soon<S: Store>(s: &Server<S>, world: ConnId, trigger: Trigger, now_unix: i64) -> bool {
    let quests = givings(s, trigger);
    quests.len() >= 2 && person_waits(s, world, &quests, now_unix)
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

/// Whether a give's step runs for so many of its item: exactly the count it wants, or whatever is
/// given when it wants none. Of 2017's 26 gives to a step naming a count, 25 gave that count and
/// were taken, a stack of more split to it first, and the other, three fins to a person wanting one
/// at a time, was refused; where no count is named, 82 stones were taken whole where 1 had been.
fn takes_count(st: &Step, quantity: u32) -> bool {
    st.wants.is_none_or(|n| n == quantity)
}

/// The step a give of so many of an item would run now, if any.
#[must_use]
pub fn step_for_give<S: Store>(
    s: &Server<S>,
    world: ConnId,
    trigger: Trigger,
    quantity: u32,
    now_unix: i64,
) -> Option<Step> {
    s.quests
        .steps
        .iter()
        .find(|st| {
            st.trigger == trigger && takes_count(st, quantity) && may_run(s, world, st, now_unix)
        })
        .cloned()
}

/// Does a template count in a class (a script package: its own, or one its package derives from)?
#[must_use]
pub fn in_class<S: Store>(s: &Server<S>, template: u32, class: u32) -> bool {
    s.catalog
        .get(template)
        .is_some_and(|t| s.item_classes.counts_in(t.description.package, class))
}

/// The triggers a give of an item of a template to one of another makes: the give of the template,
/// then the give of each class a step names for that person or object that the template counts in.
#[must_use]
pub fn give_triggers<S: Store>(s: &Server<S>, template: u32, to: u32) -> Vec<Trigger> {
    let mut out = vec![Trigger::Give(template, to)];
    for st in &s.quests.steps {
        if let Trigger::GiveClass(c, t) = st.trigger {
            if t == to && !out.contains(&st.trigger) && in_class(s, template, c) {
                out.push(st.trigger);
            }
        }
    }
    out
}

/// The give that moves no quest a person takes so many of an item by: its row for that item, else
/// one for a class the item counts in, else one for any item. A row for the item wanting another count refuses it; so does a row whose
/// requirements the player does not meet, or one that would do nothing for the player (it gives
/// nothing and its only effects are points bonuses at their caps already), unless one for any item
/// takes it.
#[must_use]
pub fn give_for<S: Store>(
    s: &Server<S>,
    world: ConnId,
    item: u32,
    to: u32,
    quantity: u32,
) -> Option<yalain_content::quests::Give> {
    let p = s.players.get(&world)?;
    let held = &p.play.quests;
    let kingdom = crate::kingdoms::counted_of(&p.qualities);
    let meets = |g: &&yalain_content::quests::Give| {
        // One whose gift can't be made takes nothing: the item goes back to its giver.
        let picked = s.item_rows.give_pick(g).unwrap_or_default();
        g.gives
            .iter()
            .chain(picked.iter().map(|(t, _)| t))
            .all(|t| crate::makes::can_make(s, *t))
            && s.kingdom_rows.give_for(g, kingdom)
            && crate::tallies::give_for(s, g)
            && crate::craft::give_allows(s, world, g)
            && crate::skills::untraining_allows_give(s, world, g)
            && crate::focus::allows_give(s, world, g)
            && !crate::kingdom_skills::would_do_nothing(s, &p.qualities, g)
            && g.requires.iter().all(|(q, need)| match need {
                Need::Phase(n) => phase_of(held, *q) == *n,
                Need::Done => done(held, *q),
                Need::Free => phase_of(held, *q) == 0,
                Need::Failed => failed_during(held, *q, None),
            })
    };
    // Each with the class it takes, if it names one.
    let rows: Vec<(&yalain_content::quests::Give, Option<u32>)> = s
        .quests
        .gives
        .iter()
        .enumerate()
        .filter(|(_, g)| g.to == to)
        .filter(|(_, g)| meets(g))
        .map(|(i, g)| (g, s.content.class_gives.get(&i).copied()))
        .collect();
    let wanting = |of: Vec<&yalain_content::quests::Give>| {
        of.into_iter()
            .find(|g| g.wants.is_none_or(|n| n == quantity))
            .cloned()
    };
    let named: Vec<&yalain_content::quests::Give> = rows
        .iter()
        .filter(|(g, _)| g.item == Some(item))
        .map(|(g, _)| *g)
        .collect();
    if !named.is_empty() {
        return wanting(named);
    }
    let of_class: Vec<&yalain_content::quests::Give> = rows
        .iter()
        .filter(|(_, c)| c.is_some_and(|c| in_class(s, item, c)))
        .map(|(g, _)| *g)
        .collect();
    if !of_class.is_empty() {
        return wanting(of_class);
    }
    rows.into_iter()
        .find(|(g, c)| g.item.is_none() && c.is_none())
        .map(|(g, _)| g.clone())
}

/// The quest of a step a give would run now but for its count, if any.
#[must_use]
pub fn wants_another_count<S: Store>(
    s: &Server<S>,
    world: ConnId,
    trigger: Trigger,
    quantity: u32,
    now_unix: i64,
) -> Option<u32> {
    s.quests
        .steps
        .iter()
        .find(|st| {
            st.trigger == trigger && !takes_count(st, quantity) && may_run(s, world, st, now_unix)
        })
        .map(|st| st.quest)
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
    // What it adds to the world's tallies comes first, so what is said with it names the count it
    // made.
    crate::tallies::step_ran(s, step, clock);
    if let Some((teller, name)) = who {
        if let Some(line) = &step.tell {
            let told = crate::presence::content_by_id(&s.content.objects, teller)
                .map(|i| crate::talk::content_speaker(s, i).told_as())
                .unwrap_or_else(|| name.map_or_else(TextInfo::default, progress::text));
            tell(s, net, world, teller, told, line);
        }
        if let Some(i) = crate::presence::content_by_id(&s.content.objects, teller) {
            let template = s.content.objects[i].template;
            let speaker = crate::talk::content_speaker(s, i);
            // The move that finishes a quest's last subquests is said as that quest's completion
            // (2017: the last element handed over, 1 of 1), and one that finishes an earlier
            // phase's as its next phase where the person has a line for that.
            let item = given.map(|g| g.template);
            let parents = finishes_parent(s, world, step).map_or_else(Vec::new, |(q, up)| {
                crate::talk::with_update(s, template, Some(i), (q, up), item)
            });
            let lines = if parents.is_empty() {
                crate::talk::with_update(s, template, Some(i), (step.quest, step.update), item)
            } else {
                parents
            };
            for l in lines {
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
                received.push(*g);
            }
        }
    }
    // One more item, of several, picked by weight.
    let pick = s.item_rows.step_pick(step).map(<[(u32, u32)]>::to_vec);
    if let Some(g) = pick.and_then(|p| crate::makes::pick_one(s, &p)) {
        match given {
            Some(from) => crate::inventory::give_from(s, net, world, g, 1, from.target),
            None => crate::inventory::give(s, net, world, g, 1),
        };
        received.push(g);
    }
    for t in &step.takes {
        if kept.is_some_and(|g| g.template == *t) {
            kept = None;
            continue;
        }
        crate::inventory::take_from_pack(s, net, world, *t);
    }
    // Its effects come before the update, or for some steps after it.
    // A step that leads somewhere itself takes the player there, not the update's own recall.
    let up = (step.quest, step.update, step.xp);
    let led = step.to.is_some();
    if step.effects_last {
        apply_led(s, net, world, up, led, clock);
        step_effects(s, net, world, step, who, clock);
    } else {
        step_effects(s, net, world, step, who, clock);
        apply_led(s, net, world, up, led, clock);
    }
    if step.xp_share > 0.0 {
        let level = s
            .players
            .get(&world)
            .map_or(1, |p| crate::examine::level_of(&p.qualities));
        let paid = share_of(&s.levels, step.xp_share, level);
        if paid > 0 {
            progress::award(s, net, world, paid, Award::Step);
        }
    }
    if step.fellows {
        for_fellows(s, net, world, step, clock);
    }
    crate::escorts::step_ran(s, net, world, step, clock);
    received
}

/// A step whose update is the fellowship's: the same update made for each of the player's fellows
/// in the world for whom the step may run, told to each alone (2017: a fellow's hoard key gave the
/// hoard's entry quest to the fellow who recorded, an update and nothing else). The items it takes
/// and gives are the player's alone.
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
    // A hand-in is the player's own: what the step takes and gives is the player's alone, and a
    // fellow needs only the phase and the quests the step requires (2017: an item handed to a
    // person, and one used up, each moved every recorded fellow's quest in the same tick, 2 of 2
    // and 3 of 3, none of them holding the item).
    let theirs = Step {
        takes: Vec::new(),
        gives: Vec::new(),
        counts: Vec::new(),
        ..step.clone()
    };
    for w in fellows {
        if may_run(s, w, &theirs, clock.unix) {
            let up = (step.quest, step.update, 0);
            apply_led(s, net, w, up, step.to.is_some(), clock);
        }
    }
}

/// A step's effects on the player (a reward, a recipe learned), cast by the player itself, then
/// what they say (a text one of them told already is not told again; none of their own lines for a
/// step whose effects are quiet), then the recipes it teaches by name; a step that transforms its
/// player makes it a hero first.
fn step_effects<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    step: &Step,
    who: Option<(u64, Option<(u32, u32)>)>,
    clock: Clock,
) {
    let me = s.players.get(&world).map_or(0, |p| p.character);
    if let Some(t) = &step.transforms {
        crate::gives::made_hero(s, net, world, t);
    }
    let mut told = Vec::new();
    for e in &step.effects {
        let cast = crate::effects::Cast {
            caster: me,
            spellcraft: 1.0,
            giver: who,
            quiet: step.effects_quiet,
            ..crate::effects::Cast::default()
        };
        told.extend(crate::effects::recipe_line(&s.effect_table, *e, cast));
        crate::effects::apply(s, net, world, *e, cast, clock);
    }
    if let Some(t) = step.text.filter(|t| !told.contains(t)) {
        progress::private(net, world, &crate::chat::show_text(2, progress::text(t)));
    }
    for r in &step.recipes {
        crate::craft::learn(s, net, world, *r, clock);
    }
}

/// The experience a quest's completion pays a player of a level: the quest's, weighed by its
/// challenge against the player's level as a kill's is by the creature's (a quest without a
/// challenge is not weighed); 2017 paid exactly this for 89 of 99 completions where the player's
/// level was known, and for all 33 vault shards, among them 9 under vitae, which changes nothing.
#[must_use]
pub fn completion_xp(xp: u64, challenge: i32, level: i32) -> u64 {
    if challenge == NO_CHALLENGE {
        return xp;
    }
    let share = crate::xp::kill_share(challenge, level).cast_unsigned();
    xp * share / 100
}

/// A share of what a level takes to reach the next, rounded down.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn share_of(levels: &LevelTable, share: f32, level: i32) -> u64 {
    (f64::from(share) * levels.to_next(level).unwrap_or(0) as f64) as u64
}

/// The experience a quest's completion pays a player of a level. A quest may pay a share of what
/// the player's level takes to reach the next, rounded down, with its limits (2017: 13 of 13
/// completions of such quests whose player's level was known, exactly, each kind of limit among them);
/// a share's limit at a level is that share of what that level takes. A quest with no share
/// pays its amount. Either is weighed by its challenge unless the quest says it is paid whole (2017:
/// where weighing would change it, the quests that say so 2 of 2 whole, the others 6 of 6 weighed).
/// A completion also adds the quest's points to the player's quest rating, whole and every time
/// (2017: of 136 pairs of a player's profiles with what it completed between them known, 133 moved
/// by exactly that, 60 of 60 in one session, the 3 others across time not recorded; of 14,336
/// profiles, none lower than the one before; no use of the rating but the profile's line is known).
#[must_use]
pub fn completion_pay(levels: &LevelTable, def: &QuestDef, level: i32) -> u64 {
    let pay = &def.pay;
    let amount = if pay.share > 0.0 {
        let mut paid = share_of(levels, pay.share, level);
        if let Some((share, at)) = pay.most_at_level {
            paid = paid.min(share_of(levels, share, at));
        }
        if pay.most > 0 {
            paid = paid.min(pay.most);
        }
        paid.max(pay.least)
    } else {
        def.xp
    };
    if pay.flat {
        amount
    } else {
        completion_xp(amount, def.challenge, level)
    }
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
    apply_led(s, net, world, (quest, up, xp), false, clock);
}

/// Make a quest update (the quest, the update, the experience the step pays) and tell the player.
/// `led`: what made it takes the player somewhere itself (a step's place, an order's), so a recall
/// among the update's effects stays put and the row's place is the one the player goes to. An
/// update whose recall sends the player is told as the player comes out of portal space.
pub fn apply_led<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    (quest, up, xp): (u32, Update, u64),
    led: bool,
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
        Update::Fail => {
            fail(s, net, world, quest, clock);
            return;
        }
        Update::Nothing => return,
    };
    // A subquest of a quest the player has at the phase it belongs to: its move is told as that
    // quest's.
    let parent = parent_held(s, world, quest);
    let recalled = update_effects(s, net, world, &def, effects_of, led, clock);
    if xp > 0 {
        progress::award(s, net, world, xp, Award::Step);
    }
    if up == Update::Complete {
        let level = s
            .players
            .get(&world)
            .map_or(1, |p| crate::examine::level_of(&p.qualities));
        let paid = completion_pay(&s.levels, &def, level);
        if paid > 0 {
            // A hidden quest's pay comes with no text (2017: the vaults' shard quests, 31 of 31).
            let why = if is_hidden(&s.quests, &def) {
                Award::Step
            } else {
                Award::Quest(def.id)
            };
            progress::award(s, net, world, paid, why);
        }
        if let Some(p) = s.players.get_mut(&world) {
            p.play.quest_rating = p.play.quest_rating.saturating_add(def.pay.points);
        }
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
        // Gone by above.
        Update::Fail | Update::Nothing => return,
    };
    let phase = st.phase;
    // A phase done by subquests gives them with it, under way at their first phase.
    if up != Update::Complete {
        give_subquests(s, world, quest, phase, clock);
    }
    if !is_hidden(&s.quests, &def) {
        let trip = s
            .players
            .get_mut(&world)
            .and_then(|p| p.trip.as_mut())
            .filter(|_| recalled);
        match trip {
            Some(t) => t.updates.push((quest, kind)),
            None => tell_record(s, net, world, &def, kind, clock),
        }
    }
    crate::world::keep_player(s, world);
    crate::escorts::quest_moved(s, net, world, quest);
    if up == Update::Bestow {
        start_timer(s, world, &def, clock);
        triggered(s, net, world, Trigger::Given(quest), None, clock);
    } else if matches!(up, Update::Advance(_)) && def.timed_phase == phase && phase > 1 {
        start_timer(s, world, &def, clock);
    }
    if let Some((parent, at)) = parent {
        subquest_moved(s, net, world, parent, at, clock);
    }
    if up == Update::Complete {
        companions_end(s, net, world, quest, true, clock);
        triggered(s, net, world, Trigger::Done(quest), None, clock);
    }
}

/// A player's quest is done (`done`), or has failed or been cancelled: the quests that go with it,
/// under way, the player's and each of its fellows' in the world, are done or fail with it.
fn companions_end<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    quest: u32,
    done: bool,
    clock: Clock,
) {
    let Some(companions) = s.content.companions.get(&quest).cloned() else {
        return;
    };
    let mut holders = vec![world];
    holders.extend(crate::fellowship::fellows_in_world(s, world));
    for w in holders {
        for &c in &companions {
            let under_way = s.players.get(&w).is_some_and(|p| {
                p.play
                    .quests
                    .iter()
                    .any(|q| q.quest == c && q.status == status::UNDERWAY)
            });
            if !under_way {
                continue;
            }
            if done {
                apply(s, net, w, c, Update::Complete, 0, clock);
            } else {
                fail(s, net, w, c, clock);
            }
        }
    }
}

/// A player out of the portal space an update's recall sent it into is told the update: its record
/// of the quest as it stands now, as an update of that kind (a quest taken off on the way is told
/// nothing).
pub(crate) fn told_on_arrival<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    quest: u32,
    kind: u32,
    clock: Clock,
) {
    if let Some(def) = s.quests.quest(quest) {
        tell_record(s, net, world, def, kind, clock);
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

/// A quest is taken off a player, its client told as it goes: whether the player held it. A quest
/// its client was never told of (a hidden one, or one the world no longer defines) is taken off
/// without a word. Its subquests go with it, as quietly.
pub fn remove<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    quest: u32,
    clock: Clock,
) -> bool {
    if let Some(def) = s.quests.quest(quest).filter(|d| !is_hidden(&s.quests, d)) {
        tell_record(s, net, world, def, update::REMOVE, clock);
    }
    let Some(p) = s.players.get_mut(&world) else {
        return false;
    };
    let before = p.play.quests.len();
    p.play.quests.retain(|st| st.quest != quest);
    let held = p.play.quests.len() != before;
    if held {
        remove_subquests(s, world, quest);
    }
    crate::escorts::quest_moved(s, net, world, quest);
    held
}

/// A quest has been taken off a player: the subquests of its phases go too, and theirs, but for one
/// another quest the player has at a phase that lists it still needs. Its client knew none of them.
fn remove_subquests<S: Store>(s: &mut Server<S>, world: ConnId, quest: u32) {
    let subs: Vec<u32> = s.quests.quest(quest).map_or_else(Vec::new, |d| {
        d.subquests
            .iter()
            .flat_map(|p| p.quests.iter().copied())
            .collect()
    });
    for sub in subs {
        if parent_held(s, world, sub).is_some() {
            continue;
        }
        let Some(p) = s.players.get_mut(&world) else {
            return;
        };
        let before = p.play.quests.len();
        p.play.quests.retain(|st| st.quest != sub);
        if p.play.quests.len() != before {
            remove_subquests(s, world, sub);
        }
    }
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

/// The quest a step's completion of its subquest would move on, and how: the quest held at the
/// phase whose subquests done with this one finish it, done from its last phase, else on to the
/// next.
fn finishes_parent<S: Store>(s: &Server<S>, world: ConnId, step: &Step) -> Option<(u32, Update)> {
    if step.update != Update::Complete {
        return None;
    }
    let (parent, at) = parent_held(s, world, step.quest)?;
    let def = s.quests.quest(parent)?;
    let subs = s.quests.subquests(parent, at)?;
    let last = usize::try_from(at).unwrap_or(usize::MAX) >= def.journals.len();
    let held = &s.players.get(&world)?.play.quests;
    let mut others = subs
        .quests
        .iter()
        .map(|q| *q == step.quest || done(held, *q));
    let up = if last {
        Update::Complete
    } else {
        Update::Advance(at + 1)
    };
    (subs.any || others.all(|d| d)).then_some((parent, up))
}

/// A subquest of a quest's phase has moved: once all of the phase's subquests are done (one,
/// where one is enough) the quest moves on to its next phase; until then its record is told again,
/// its phase kept (2017: the third doctrine recorded moved the quest to its second phase, the first
/// two told with the phase kept, 9 of 9). From a last phase it is done.
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
    // A last phase done by its subquests is the quest done: of 2017's completions of such quests,
    // each came with the move that finished the last of them.
    if through {
        apply(s, net, world, quest, Update::Complete, 0, clock);
        return;
    }
    if !is_hidden(&s.quests, &def) {
        tell_record(s, net, world, &def, update::ADVANCE, clock);
    }
    crate::world::keep_player(s, world);
}

/// A player dies: its quests under way that fail when their player dies fail (2017: the Mist
/// Opal's, 2 of 2, in the death's tick before the death's message).
pub fn died<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    for q in under_way_failing(s, world, |d| d.fails_on_death) {
        fail(s, net, world, q, clock);
    }
}

/// A player is in portal space on its way somewhere (not back from its death): its quests under way
/// that fail when their player is taken through portal space fail. Called once the trip is the
/// player's, so an effect of a failure that would send it somewhere leaves it to the trip it is on.
pub fn teleported<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    for q in under_way_failing(s, world, |d| d.fails_on_teleport) {
        fail(s, net, world, q, clock);
    }
}

/// A character is leaving the world (its logging off at its end, or going out of the world): its
/// quests under way that fail when their player leaves fail, while its fellowship still holds it.
pub fn leaves_world<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    for q in under_way_failing(s, world, |d| d.fails_on_logout) {
        fail(s, net, world, q, clock);
    }
}

/// The quests under way that fail when their player leaves the world, still under way as the
/// character enters (the server stopped before it saw the character leave), fail as of `left_unix`,
/// when it was last kept.
pub fn failed_on_leaving(
    table: &yalain_content::QuestTable,
    held: &mut [QuestState],
    left_unix: i64,
) {
    for st in held.iter_mut().filter(|q| q.status == status::UNDERWAY) {
        if table.quest(st.quest).is_some_and(|d| d.fails_on_logout) {
            st.status = status::FAILED;
            st.phase = 0;
            st.updated = left_unix;
        }
    }
}

/// The quests a player holds under way whose definitions `fails` says fail now.
fn under_way_failing<S: Store>(
    s: &Server<S>,
    world: ConnId,
    fails: impl Fn(&QuestDef) -> bool,
) -> Vec<u32> {
    s.players.get(&world).map_or_else(Vec::new, |p| {
        p.play
            .quests
            .iter()
            .filter(|q| q.status == status::UNDERWAY)
            .filter(|q| s.quests.quest(q.quest).is_some_and(&fails))
            .map(|q| q.quest)
            .collect()
    })
}

/// Put on a player the effects a quest's definition names for an update of this kind, cast by the
/// player itself; the first recall among them takes the player, unless the update is `led`.
/// Returns whether one did.
fn update_effects<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    def: &QuestDef,
    kind: u32,
    led: bool,
    clock: Clock,
) -> bool {
    let me = s.players.get(&world).map_or(0, |p| p.character);
    let mut recalled = false;
    for (_, e, spellcraft) in def.effects.iter().filter(|(k, _, _)| *k == kind) {
        if !led && !recalled {
            recalled = crate::skill_use::effect_recall(s, net, world, *e, clock);
        }
        crate::effects::apply(
            s,
            net,
            world,
            *e,
            crate::effects::Cast {
                caster: me,
                spellcraft: *spellcraft,
                ..crate::effects::Cast::default()
            },
            clock,
        );
    }
    recalled
}

/// The timer that fails a quest of a player's when its time runs out, if its time is running.
fn start_timer<S: Store>(s: &mut Server<S>, world: ConnId, def: &QuestDef, clock: Clock) {
    let Some(st) = s
        .players
        .get(&world)
        .and_then(|p| p.play.quests.iter().find(|q| q.quest == def.id))
        .copied()
    else {
        return;
    };
    runs_out_after(s, world, def, &st, clock);
}

/// When a quest under way runs out, if its time is running: the timer that fails it.
fn runs_out_after<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    def: &QuestDef,
    st: &QuestState,
    clock: Clock,
) {
    let Some(from) = time_since(def, st) else {
        return;
    };
    #[allow(clippy::cast_precision_loss)]
    let left = (from as f64 + def.duration - clock.unix as f64).max(0.0);
    s.after(
        clock,
        left,
        crate::Timer::QuestRunsOut {
            world,
            quest: def.id,
            started: st.started,
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
            runs_out_after(s, world, &def, st, clock);
        }
    }
}

/// A character enters after `away` seconds out of the world: its time did not run. Each quest it
/// holds counts from `away` later, done or not, as a person's wait counts from it too (2017: when
/// each was given, as an entry listed it, moved on by the time left's fall short of the time that
/// passed, for every quest held). Never past now, for a quest given while it was out of the world.
pub fn time_stood_still(held: &mut [QuestState], away: i64, now_unix: i64) {
    for st in held {
        st.started = st
            .started
            .saturating_add(away)
            .min(now_unix.max(st.started));
        if st.status == status::UNDERWAY {
            st.updated = st
                .updated
                .saturating_add(away)
                .min(now_unix.max(st.updated));
        }
    }
}

/// The quests with no time left when their player last left the world, at `left_unix`, fail, as of
/// when it ran out: a server that stopped before their time told it (2017: no entry listed a quest
/// under way with no time left, 691 of 691 timed ones with time left).
pub fn ran_out_before_leaving(
    table: &yalain_content::QuestTable,
    held: &mut [QuestState],
    left_unix: i64,
) {
    for st in held.iter_mut().filter(|q| q.status == status::UNDERWAY) {
        let Some(def) = table.quest(st.quest) else {
            continue;
        };
        let Some(from) = time_since(def, st) else {
            continue;
        };
        #[allow(clippy::cast_possible_truncation)]
        let ends = from + def.duration.ceil() as i64;
        if ends <= left_unix {
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
    // Still that giving of it, and its time still running: a phase its duration does not run in has
    // come if not.
    let running = s.quests.quest(quest).is_some_and(|def| {
        s.players.get(&world).is_some_and(|p| {
            p.play
                .quests
                .iter()
                .any(|q| q.quest == quest && q.started == started && time_since(def, q).is_some())
        })
    });
    if running {
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
    let recalled = update_effects(s, net, world, &def, update::FAIL, false, clock);
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
        let trip = s
            .players
            .get_mut(&world)
            .and_then(|p| p.trip.as_mut())
            .filter(|_| recalled);
        if let Some(t) = trip {
            t.updates.push((quest, update::FAIL));
        } else {
            let rec = record(&def, &st, game_now, game_epoch);
            progress::private(net, world, &update_call(rec, update::FAIL));
        }
    }
    crate::world::keep_player(s, world);
    crate::escorts::quest_moved(s, net, world, quest);
    companions_end(s, net, world, quest, false, clock);
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
    crate::escorts::quest_moved(s, net, world, quest);
    companions_end(s, net, world, quest, false, clock);
}
