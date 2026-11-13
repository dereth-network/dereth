//! Objects whose state the items given them change, and uses in an order judged against those
//! states.
//!
//! **States.** The world's objects of some templates each hold a state (the world's
//! `[[object_states]]` rows), at the start the one its row gives its template. An item given to
//! such an object, where one of the row's changes names the item and the state the object is in,
//! puts the object in the change's state and is handed back: a change uses nothing up. The object
//! goes back to its first state the row's time after its last change. The state is the object's,
//! the same for every player. An item no change names is answered as any other give.
//!
//! **Orders of uses.** A `[[sequence]]` row names uses in order, each of a template and in the
//! state its object must be in, and a quest whose phase counts the right uses a player has made: a
//! use of the next in the state named moves the quest on a phase (the first gives it), with what
//! the quest's definition names for that update, and the last takes the player through portal
//! space to the row's place, if it names one, as a step's use of an object does; a use of any other object the row names, or of
//! the next in another state, takes the quest away from a player who has it under way, and the
//! order starts over. Every row naming the object used is judged at each use, so one use may move
//! several on and start others over. A row whose quest is past its last use, or done and not to be
//! given again, is not judged. The object says the right line of the first row a use moved on,
//! else the wrong line of the first row it judged that has one, else nothing; a use of an object a
//! row names is answered by nothing else.
//!
//! **By placed object.** A row's use may name one placed object instead of a template, so several
//! objects of one template stand in an order of their own: the use fits only that object, and a
//! use of another of its template is another object's. A row may also start over only for an
//! object already used in the order: a use of one further on is told the row's wrong line and
//! changes nothing, so the order may be found by trying.
//!
//! **A count of its own.** A row may name no quest: it then keeps its own count of the right uses
//! made, one for each fellowship (every fellow's use counts in it, and a fellow's wrong use starts
//! it over for them all) and one for each player in none. A right use moves the count on, a wrong
//! one starts it over as a quest's would be taken away, and once every use is made the row judges
//! no more uses for them until the count is forgotten: its row's time after its last right use,
//! done or not. A quest step or a give may wait on such a count, at so many right uses or at all of
//! them, read for the player's fellowship or, in none, the player. The counts are not kept across
//! a restart; a count at or past every use a reloaded row names reads as done.

use std::collections::HashMap;

use yalain_content::item_rows::{ObjectStates, Sequence};
use yalain_content::quests::Update;
use yalain_content::Words;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::{presence, Clock, Server};

/// The objects whose state has changed since the start: each one's state and when it changed, in
/// the transport's seconds. An object not here is in its first state. And the counts orders of uses
/// keep of their own: the right uses made and when the last was, by the row's key and whose count
/// it is. A count not here is none made.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct States {
    pub changed: HashMap<u64, (String, f64)>,
    pub counts: HashMap<(String, Counted), (u32, f64)>,
}

/// Whose count an order of uses keeps: a fellowship's (by its chat room, its own while it lasts),
/// or a player's in none (by its character).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Counted {
    Fellowship(u32),
    Player(u64),
}

/// Whose count a player's uses move and its steps read: its fellowship's, else its own.
#[must_use]
pub fn counted<S: Store>(s: &Server<S>, world: ConnId) -> Option<Counted> {
    let me = s.players.get(&world)?.character;
    Some(match s.fellowships.of(me) {
        Some(i) => Counted::Fellowship(s.fellowships.list[i].room),
        None => Counted::Player(me),
    })
}

/// The right uses an order keeping its own count stands at for whoever's count it is, `now` (in the
/// transport's seconds): none once its row's time has passed since the last.
#[must_use]
pub fn count<S: Store>(s: &Server<S>, row: &Sequence, whose: Counted, now: f64) -> u32 {
    match s.object_states.counts.get(&(row.key.clone(), whose)) {
        Some((n, at)) if row.forgets_after.is_none_or(|f| now - at < f) => *n,
        _ => 0,
    }
}

/// Does an order keeping its own count stand where a step's or a give's gate needs it, for the
/// player's count? An order the world no longer holds stands nowhere.
#[must_use]
pub fn gate_holds<S: Store>(
    s: &Server<S>,
    world: ConnId,
    gate: &yalain_content::item_rows::SequenceGate,
) -> bool {
    use yalain_content::item_rows::SequenceNeed;
    let Some(row) = s.item_rows.sequences.iter().find(|q| q.key == gate.key) else {
        return false;
    };
    let Some(whose) = counted(s, world) else {
        return false;
    };
    let now = s.last_step.map_or(0.0, |t| t.0);
    let all = u32::try_from(row.uses.len()).unwrap_or(u32::MAX);
    let n = count(s, row, whose, now).min(all);
    match gate.need {
        SequenceNeed::At(want) => n == want,
        SequenceNeed::Done => n == all,
    }
}

/// May a step run for a player, by the orders of uses it waits on?
#[must_use]
pub fn allows_step<S: Store>(
    s: &Server<S>,
    world: ConnId,
    step: &yalain_content::quests::Step,
) -> bool {
    s.item_rows.sequence_steps.is_empty()
        || s.item_rows
            .step_sequences(step)
            .all(|g| gate_holds(s, world, g))
}

/// May a give take its item from a player, by the orders of uses it waits on?
#[must_use]
pub fn allows_give<S: Store>(
    s: &Server<S>,
    world: ConnId,
    give: &yalain_content::quests::Give,
) -> bool {
    s.item_rows.sequence_gives.is_empty()
        || s.item_rows
            .give_sequences(give)
            .all(|g| gate_holds(s, world, g))
}

/// The state an object of the world is in now, if its template holds one.
#[must_use]
pub fn state_of<S: Store>(s: &Server<S>, object: u64, clock: Clock) -> Option<String> {
    let i = presence::content_by_id(&s.content.objects, object)?;
    let template = s.content.objects[i].template;
    let row = s.item_rows.states_of(template)?;
    Some(current(s, row, template, object, clock))
}

/// An object's state by its row: the last change's while it lasts, else the template's first.
fn current<S: Store>(
    s: &Server<S>,
    row: &ObjectStates,
    template: u32,
    object: u64,
    clock: Clock,
) -> String {
    let first = || {
        row.templates
            .iter()
            .find(|(t, _)| *t == template)
            .map_or_else(String::new, |(_, f)| f.clone())
    };
    match s.object_states.changed.get(&object) {
        Some((state, at)) if row.reverts.is_none_or(|r| clock.local.0 - at < r) => state.clone(),
        _ => first(),
    }
}

/// An item given to an object of the world: where a change names the item and the object's state,
/// the object takes the change's state. Returns whether it did (the item is then handed back).
pub fn given<S: Store>(s: &mut Server<S>, object: u64, item: u32, clock: Clock) -> bool {
    let Some(i) = presence::content_by_id(&s.content.objects, object) else {
        return false;
    };
    let template = s.content.objects[i].template;
    let Some(row) = s.item_rows.states_of(template) else {
        return false;
    };
    let now = current(s, row, template, object, clock);
    let Some(to) = row
        .changes
        .iter()
        .find(|c| c.item == item && c.from == now)
        .map(|c| c.to.clone())
    else {
        return false;
    };
    s.object_states.changed.insert(object, (to, clock.local.0));
    true
}

/// Does an order of uses judge a use of this object's template?
#[must_use]
pub fn judges<S: Store>(s: &Server<S>, template: u32) -> bool {
    s.item_rows.in_a_sequence(template)
}

/// A player has used an object some order of uses names: each such row is judged, its quest moved
/// on a phase, given, or taken away, and the object says its line.
pub fn used<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    object: u64,
    clock: Clock,
) {
    let Some(i) = presence::content_by_id(&s.content.objects, object) else {
        return;
    };
    let template = s.content.objects[i].template;
    let state = state_of(s, object, clock);
    let rows: Vec<Sequence> = s
        .item_rows
        .sequences
        .iter()
        .filter(|q| {
            q.uses.iter().enumerate().any(|(k, (t, _))| {
                *t == template && q.objects.get(k).copied().flatten().is_none_or(|o| o == i)
            })
        })
        .cloned()
        .collect();
    let mut right = None;
    let mut wrong = None;
    for q in &rows {
        let Some(quest) = q.quest else {
            let (r, w) = own_count_used(s, world, (object, i), q, clock);
            right = right.or(r);
            wrong = wrong.or(w);
            continue;
        };
        let Some(def) = s.quests.quest(quest).cloned() else {
            continue;
        };
        let Some(held) = s.players.get(&world).map(|p| p.play.quests.clone()) else {
            return;
        };
        let at = crate::quests::phase_of(&held, quest);
        if at == 0 && !crate::quests::given_again(&def, &held, clock.unix) {
            continue;
        }
        let Some(n) = usize::try_from(at).ok() else {
            continue;
        };
        let Some((_, wants)) = q.uses.get(n) else {
            continue;
        };
        // The use of this object at a place in the order: its own, or any of its template's.
        let is = |k: usize| {
            q.uses[k].0 == template && q.objects.get(k).copied().flatten().is_none_or(|o| o == i)
        };
        let fits = is(n) && wants.as_ref().is_none_or(|w| state.as_ref() == Some(w));
        wrong = wrong.or(q.wrong);
        // An object further on, where only a used one starts the order over: told, nothing moved.
        if !fits && !q.wrong_starts_over && !(0..n).any(is) {
            continue;
        }

        if fits {
            let up = if at == 0 {
                Update::Bestow
            } else {
                Update::Advance(at + 1)
            };
            let last = usize::try_from(at + 1).is_ok_and(|n| n == q.uses.len());
            // A last use whose row names a place takes the player there, not the update's recall.
            let led = last && q.to.is_some();
            crate::quests::apply_led(s, net, world, (quest, up, 0), led, clock);
            right = right.or(Some(q.right));
            if let Some(place) = q.to.filter(|_| last) {
                let place = crate::quests::room_for(s, place);
                crate::places::sends_on(s, world, object, place, None, clock);
            }
        } else if at != 0 {
            crate::quests::remove(s, net, world, quest, clock);
        }
    }
    let line = match right {
        Some(r) => r,
        None => wrong,
    };
    if let Some((string, table)) = line {
        let speaker = crate::talk::content_speaker(s, i);
        crate::quests::tell(
            s,
            net,
            world,
            speaker.id,
            speaker.told_as(),
            &Words::Ref(string, table),
        );
    }
}

/// The lines a use judged by an order says: the right one, where it moved the order on (the row's,
/// which may be none), and the wrong one, where it was judged and did not.
type Said = (Option<Option<(u32, u32)>>, Option<(u32, u32)>);

/// A use of an object an order keeping its own count names, judged for the user's count: a right
/// use moves it on (the last takes the user to the row's place, if it names one), a wrong one starts
/// it over or, for one further on where only a used one starts over, is only told; a count already
/// done is not judged.
fn own_count_used<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    (object, i): (u64, usize),
    q: &Sequence,
    clock: Clock,
) -> Said {
    let Some(whose) = counted(s, world) else {
        return (None, None);
    };
    let at = count(s, q, whose, clock.local.0);
    let Some(n) = usize::try_from(at).ok().filter(|n| *n < q.uses.len()) else {
        return (None, None);
    };
    let template = s.content.objects[i].template;
    let state = state_of(s, object, clock);
    let is = |k: usize| {
        q.uses[k].0 == template && q.objects.get(k).copied().flatten().is_none_or(|o| o == i)
    };
    let fits = is(n)
        && q.uses[n]
            .1
            .as_ref()
            .is_none_or(|w| state.as_ref() == Some(w));
    let key = (q.key.clone(), whose);
    if fits {
        s.object_states.counts.insert(key, (at + 1, clock.local.0));
        if let Some(place) = q.to.filter(|_| n + 1 == q.uses.len()) {
            let place = crate::quests::room_for(s, place);
            crate::places::sends_on(s, world, object, place, None, clock);
        }
        return (Some(q.right), None);
    }
    if q.wrong_starts_over || (0..n).any(is) {
        s.object_states.counts.remove(&key);
    }
    (None, q.wrong)
}
