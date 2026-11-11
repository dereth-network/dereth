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

use std::collections::HashMap;

use yalain_content::item_rows::{ObjectStates, Sequence};
use yalain_content::quests::Update;
use yalain_content::Words;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::{presence, Clock, Server};

/// The objects whose state has changed since the start: each one's state and when it changed, in
/// the transport's seconds. An object not here is in its first state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct States {
    pub changed: HashMap<u64, (String, f64)>,
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
        .filter(|q| q.uses.iter().any(|(t, _)| *t == template))
        .cloned()
        .collect();
    let mut right = None;
    let mut wrong = None;
    for q in &rows {
        let Some(def) = s.quests.quest(q.quest).cloned() else {
            continue;
        };
        let Some(held) = s.players.get(&world).map(|p| p.play.quests.clone()) else {
            return;
        };
        let at = crate::quests::phase_of(&held, q.quest);
        if at == 0 && !crate::quests::given_again(&def, &held, clock.unix) {
            continue;
        }
        let Some((next, wants)) = usize::try_from(at).ok().and_then(|n| q.uses.get(n)) else {
            continue;
        };
        let fits = *next == template && wants.as_ref().is_none_or(|w| state.as_ref() == Some(w));
        wrong = wrong.or(q.wrong);
        if fits {
            let up = if at == 0 {
                Update::Bestow
            } else {
                Update::Advance(at + 1)
            };
            let last = usize::try_from(at + 1).is_ok_and(|n| n == q.uses.len());
            // A last use whose row names a place takes the player there, not the update's recall.
            let led = last && q.to.is_some();
            crate::quests::apply_led(s, net, world, (q.quest, up, 0), led, clock);
            right = right.or(Some(q.right));
            if let Some(place) = q.to.filter(|_| last) {
                let place = crate::quests::room_for(s, place);
                crate::places::sends_on(s, world, object, place, None, clock);
            }
        } else if at != 0 {
            crate::quests::remove(s, net, world, q.quest, clock);
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
