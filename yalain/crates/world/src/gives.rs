//! Gives: an item of the pack dragged onto a person or an object of the world.
//!
//! **Asked.** The client asks to move the item to the person (move type 36). A player already
//! busy (another give, a drop, a use under way) is refused at once, actions locked; so is a give
//! to a person (not an object) another player is giving something to (target busy: 2017, 1 of 1;
//! eight gives to an object during another's were all taken). Otherwise the player is walked to
//! the person (a movement order to 2 m, sent even when it is there: 360 of 360). A person is busy
//! only with a give being handed to it: one still on its way, from however far, holds no one up.
//!
//! **Handed over.** In reach, the walk stops, the item's container is cleared, and the player plays
//! its handing-over behavior with the item's look in its hand, in the walk's context, sent in that
//! order (2017: the stop before the container in 224 of 224, the container before the behavior in
//! 288 of 288); 1.31 s later (the median of 360) the move is answered as done, the animation
//! played. A walk that ends out of reach is refused as one that could not get there, nothing moved
//! (5 of 5).
//!
//! **What the person does with it.** The quest steps the item given to that person triggers run
//! (each quest at most once). A step that takes the item keeps it: the item leaves the pack and the
//! world (destroyed in the person), then the person's lines, the items it gives (each made from the
//! person), the quest's update, as a use's step does (2017: 260 kept). An item a person gives that
//! moves another step of the same quest when received moves it on after the step's own update (the
//! Land Claim: two records in one instant, 1 of 1). A step that keeps nothing hands the item back
//! after its update (23 of 23). With no step for it the person says its own refusal of a give and
//! hands it back (30 of 30): a person with a body plays its handing-over behavior and the item is
//! back 1.30 s later, an object hands it back at once. An item that changes the state of an object
//! holding one ([`crate::puzzles`]) is handed back as it changes it. An item of the set an object's
//! offering takes ([`crate::offerings`]) is kept by it, or refused when it holds one already. A
//! seed given to its species' garden is planted there ([`crate::plants`]).
//!
//! **Stacks.** What is given is taken whole when a step keeps it: the count asked, a part of a
//! stack leaving the rest in the pack (2017: 38 of 38). A step may want an exact count: any other
//! is refused as an item not wanted, with the person's line for it if it has one, and handed back,
//! the quest not moved (2017: three fins to a person wanting one at a time, 1 of 1; of the gives to a step
//! naming a count, the other 25 gave that count and were taken). A part of a stack never leaves the pack: its container is not
//! cleared at the handing over (2017: 25 of 27) nor told again when it comes back.
//!
//! **A creature takes nothing.** A give to a creature (a pet, its owner's or another's, or a
//! monster) is walked to and stopped as any other. An attuned item is refused there, nothing handed
//! over (2017: 1 of 1). Anything else is handed over, and 1.31 s later refused as an item it is not
//! accepting, sent in this order: the item back in its pack, the answer as for an item taken but
//! still merging, then the player handing it over again (2017: 2 of 2, both to a pet).
//!
//! **The hero's transformation.** A step that transforms its player runs its transformation's time
//! after the answer (2017: 25.59 s, the one hero, 1 of 1), the transformation's appearances and
//! behavior played on the player at the answer; as it runs, the player is made a hero and told its
//! text.

use auberean_primitives::Vec3;
use auberean_protocol::physics::{Fx, ObjectEvent, ObjectOnly};
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::script::{ClientCall, FunctionId, ObjectCall, ScriptArgs};
use auberean_protocol::types::{BehaviorParams, MoveTarget, MovementParams, ObjectStamp};
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::quests::{Step, Transformation};
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::inventory::{move_done, q, Move};
use crate::{presence, progress, Clock, Server, Timer};

/// The move type of a give.
pub const GIVE: u32 = 36;

/// How near the person the player is walked to hand an item over, in meters, and the walk's
/// movement order flags.
pub const REACH: f32 = 2.0;
pub const WALK_FLAGS: u32 = 0x61;

/// The handing-over behavior (the giver's, and a person's handing an item back).
pub const HANDING: u32 = 0x4100_00FA;

/// How long after the handing-over behavior the give is answered, and after a person's handing
/// back the item is back, in seconds (2017: the medians of 360 and of 86).
pub const HANDED_AFTER: f64 = 1.31;
pub const HANDED_BACK_AFTER: f64 = 1.30;

/// Why a give was refused, as its answer says.
pub mod status {
    /// The walk ended out of reach.
    pub const CANT_GET_THERE: u32 = 57;
    /// The giver is busy.
    pub const ACTIONS_LOCKED: u32 = 1029;
    /// The person is taking another player's give.
    pub const TARGET_BUSY: u32 = 1030;
    /// The item is attuned to its holder.
    pub const ATTUNED: u32 = crate::drops::status::ATTUNED;
    /// What it is given to is not accepting items (a creature).
    pub const NOT_ACCEPTING: u32 = 1054;
}

/// The call that tells a client an item in a container is gone (sent as the container's).
pub const DESTROY_CONTAINED: u32 = 0x820E_005C;

/// A give under way.
#[derive(Debug, Clone, PartialEq)]
pub struct Giving {
    /// The move asked.
    pub asked: Move,
    /// The movement order's context (the handing-over plays in it).
    pub context: u32,
    /// The item has been handed over: the answer is due.
    pub handed: bool,
    /// It goes to a creature, which takes nothing.
    pub to_creature: bool,
}

/// An item given that a step took: its id, its template and the person it went to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Given {
    pub item: u64,
    pub template: u32,
    pub quantity: u32,
    pub target: u64,
}

/// The give refused at once: its own description answered with why.
fn refuse(net: &mut ServerNet, world: ConnId, m: &Move, why: u32) {
    let answer = Move {
        status: why,
        check_take: false,
        ..m.clone()
    };
    progress::private(net, world, &move_done(&answer));
}

/// The content object a give goes to, and whether it is an object (no body of its own).
fn target<S: Store>(s: &Server<S>, id: u64) -> Option<(usize, bool)> {
    let i = crate::events::content_standing(s, id)?;
    Some((i, s.content.objects[i].kind == yalain_content::Kind::Static))
}

/// Is this a creature standing in the world, alive: a give's target that takes nothing?
fn creature<S: Store>(s: &Server<S>, id: u64) -> bool {
    s.things
        .get(id)
        .is_some_and(|t| t.alive() && t.at.is_some())
}

/// How far a player standing at `at` is from what it gives to, by the walk's measure (from the
/// body's side when the body is known); none when that is gone.
fn gap_to<S: Store>(s: &Server<S>, at: &StoredPosition, id: u64) -> Option<Option<f32>> {
    let (template, scale, cell, origin) = if let Some((i, _)) = target(s, id) {
        let o = &s.content.objects[i];
        (o.template, o.description.scale, o.cell, o.origin)
    } else {
        let t = s.things.get(id).filter(|_| creature(s, id))?;
        let there = t.at?;
        (t.template, t.description.scale, there.cell, there.origin)
    };
    Some(crate::places::walk_gap(
        s, at, template, scale, cell, origin,
    ))
}

/// A client asks to give an item to a person, an object or a creature.
pub fn asked<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    m: Move,
    clock: Clock,
) {
    let content = target(s, m.target);
    let to_creature = content.is_none() && creature(s, m.target);
    if content.is_none() && !to_creature {
        s.unhandled.note(format!("a give to {:#x}", m.target));
        return;
    }
    let object = content.is_some_and(|(_, object)| object);
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let held = p
        .play
        .items
        .iter()
        .any(|h| h.id == m.item && h.location == 0);
    if !held || s.things.get(m.item).is_none() {
        s.unhandled.note("giving an item the pack does not hold");
        refuse(net, world, &m, status::CANT_GET_THERE);
        return;
    }
    if p.giving.is_some() || crate::places::actions_locked(s, world) {
        refuse(net, world, &m, status::ACTIONS_LOCKED);
        return;
    }
    let busy = !object
        && !to_creature
        && s.players.iter().any(|(w, o)| {
            *w != world
                && o.giving
                    .as_ref()
                    .is_some_and(|g| g.handed && g.asked.target == m.target)
        });
    if busy {
        refuse(net, world, &m, status::TARGET_BUSY);
        return;
    }
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.move_to_stamp = p.move_to_stamp.wrapping_add(1);
    let walk = crate::places::move_order(
        p,
        MovementParams {
            flags: WALK_FLAGS,
            desired_velocity: 1.0,
            fail_distance: f32::MAX,
            desired_distance: REACH,
            max_success_distance: REACH,
            min_success_distance: 0.0,
            target: MoveTarget::Object(m.target),
            offset: Vec3::ZERO,
            offset_id: 0,
            waypoints: Vec::new(),
            context,
        },
    );
    p.walk_rest = None;
    p.giving = Some(Giving {
        asked: m,
        context,
        handed: false,
        to_creature,
    });
    let me = p.character;
    crate::places::to_all_knowing(s, net, me, &walk);
    moved(s, net, world, clock);
}

/// A player walked to give an item has reported its place: in reach, it hands the item over; at
/// rest out of reach where it last stood still, it could not get there and the give is refused.
pub fn moved<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let Some(g) = p.giving.as_ref().filter(|g| !g.handed) else {
        return;
    };
    // What it walks to has gone (a creature that died or left): it cannot get there.
    let Some(gap) = gap_to(s, &p.position, g.asked.target) else {
        cannot_get_there(s, net, world);
        return;
    };
    let still = p.heading_motion & 0x00FF_FFFF == crate::world::IDLE_MOTION & 0x00FF_FFFF;
    let slack = REACH
        + if still {
            crate::places::STILL_SLACK
        } else {
            crate::places::WALK_SLACK
        };
    if gap.is_some_and(|d| d <= slack) {
        hand_over(s, net, world, clock);
        return;
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let stuck = still
        && p.walk_rest
            .is_some_and(|r| r.cell == p.position.cell && r.origin == p.position.origin);
    p.walk_rest = still.then_some(p.position);
    if stuck {
        cannot_get_there(s, net, world);
    }
}

/// The walk to give ends where the player is: the give is refused as one that could not get
/// there, nothing moved.
fn cannot_get_there<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(g) = p.giving.take() else {
        return;
    };
    p.walk_rest = None;
    crate::places::stop_walking(s, net, world);
    let no = Move {
        ignore_attunement: true,
        ..g.asked
    };
    refuse(net, world, &no, status::CANT_GET_THERE);
}

/// In reach: the walk stops, the item's container is cleared, then the player hands the item over
/// (seen by all who see it); the answer comes when the behavior is through. An attuned item given
/// to a creature is refused here, nothing handed over.
fn hand_over<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    crate::places::stop_walking(s, net, world);
    let attuned = s
        .players
        .get(&world)
        .and_then(|p| p.giving.as_ref())
        .is_some_and(|g| {
            g.to_creature
                && s.things
                    .get(g.asked.item)
                    .is_some_and(|t| crate::inventory::attuned_now(s, t))
        });
    if attuned {
        if let Some(g) = s.players.get_mut(&world).and_then(|p| p.giving.take()) {
            refuse(net, world, &g.asked, status::ATTUNED);
        }
        return;
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(g) = p.giving.as_mut() else {
        return;
    };
    g.handed = true;
    let (item, context, asked) = (g.asked.item, g.context, g.asked.quantity);
    let me = p.character;
    let stack = s.things.get(item).map_or(1, |t| t.quantity.max(1));
    let handing = handing_behavior(item, crate::inventory::hand_look(s, item), context);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let behave = ServerMessage::PlayBehavior(crate::places::player_event(p, handing));
    if asked >= stack {
        let cleared = ServerMessage::UpdateInstanceIdVisual(VisualQuality {
            object: crate::inventory::visual(s, item),
            quality: q::CONTAINER,
            value: 0,
        });
        crate::inventory::to_holder(s, net, world, item, &cleared);
    }
    crate::combat::to_all_seeing(s, net, me, &behave);
    s.after(clock, HANDED_AFTER, Timer::Handed { world });
}

/// The handing-over behavior with a copy of an item in the hand: the entity it is drawn as and its
/// appearance.
fn handing_behavior(item: u64, (look, appearance): (u32, u32), context: u32) -> BehaviorParams {
    let mut b = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR
            | BehaviorParams::TARGET
            | 0x100
            | BehaviorParams::CLONE
            | BehaviorParams::CONTEXT,
        behavior: HANDING,
        target: item,
        clone_visual: look,
        clone_appearance: appearance,
        context,
        ..BehaviorParams::default()
    };
    // The word of flag 0x100 (the seventh of the flags followed by a word).
    b.words[6] = 1;
    b
}

/// The answer to a give handed over: the move done, the animation played, the item named as it is
/// drawn.
fn answer(m: &Move, template: u32, (look, appearance): (u32, u32), whole: bool) -> Move {
    Move {
        merge: false,
        check_take: false,
        ignore_attunement: true,
        quantity_left: 0,
        template,
        look_entity: look,
        item_appearance: appearance,
        played_anim: true,
        all_taken: whole,
        split_quantity: if whole { 0 } else { m.quantity },
        status: 0,
        ..m.clone()
    }
}

/// A creature does not accept the item handed to it: the item is back in its pack (a part of a
/// stack never left it), then the move is refused, answered as one taken but still merging (the
/// count told when more than one), then the player hands the item over again.
#[allow(clippy::too_many_arguments)]
fn not_accepted<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    m: &Move,
    template: u32,
    (look, appearance): (u32, u32),
    whole: bool,
    context: u32,
) {
    let no = Move {
        status: status::NOT_ACCEPTING,
        check_take: false,
        template,
        look_entity: look,
        item_appearance: appearance,
        played_anim: true,
        split_quantity: if m.quantity > 1 { m.quantity } else { 0 },
        ..m.clone()
    };
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let me = p.character;
    let handing = handing_behavior(m.item, (look, appearance), context);
    let behave = ServerMessage::PlayBehavior(crate::places::player_event(p, handing));
    if whole {
        let back = ServerMessage::UpdateInstanceIdVisual(VisualQuality {
            object: crate::inventory::visual(s, m.item),
            quality: q::CONTAINER,
            value: me,
        });
        crate::inventory::to_holder(s, net, world, m.item, &back);
    }
    progress::private(net, world, &move_done(&no));
    crate::combat::to_all_seeing(s, net, me, &behave);
}

/// Which script packages derive from which: each package with the packages it derives from, so a
/// template counts in a class when its package is the class or derives from it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemClasses {
    pub bases: std::collections::HashMap<u32, Vec<u32>>,
}

impl ItemClasses {
    /// Does a template of this package count in this class?
    #[must_use]
    pub fn counts_in(&self, package: u32, class: u32) -> bool {
        package == class || self.bases.get(&package).is_some_and(|b| b.contains(&class))
    }
}

/// The handing-over is through: the give is answered, then what the person does with the item.
pub fn handed<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(g) = p.giving.take() else {
        return;
    };
    let m = g.asked;
    let held = p.play.items.iter().any(|h| h.id == m.item);
    let Some(t) = s.things.get(m.item).filter(|_| held) else {
        return;
    };
    let (template, quantity) = (t.template, t.quantity.max(1));
    let look = crate::inventory::hand_look(s, m.item);
    let whole = m.quantity >= quantity;
    if g.to_creature {
        not_accepted(s, net, world, &m, template, look, whole, g.context);
        return;
    }
    progress::private(net, world, &move_done(&answer(&m, template, look, whole)));
    let Some((i, object)) = target(s, m.target) else {
        return;
    };
    let target_template = s.content.objects[i].template;
    let given = Given {
        item: m.item,
        template,
        quantity: m.quantity.min(quantity).max(1),
        target: m.target,
    };
    // An item of a set the object wants another of is refused and handed back.
    if crate::wanting::judge(s, i, template) == crate::wanting::Judged::Refused {
        crate::wanting::refused(s, net, world, i);
        hand_back(s, net, world, i, &m, object, None, clock);
        return;
    }
    // The give of the item's own template first, then of each class it counts in.
    let triggers = crate::quests::give_triggers(s, template, target_template);
    let found = triggers.iter().find_map(|t| {
        crate::quests::step_for_give(s, world, *t, given.quantity, clock.unix).map(|st| (*t, st))
    });
    if found.is_some() {
        crate::wanting::met(s, i, template);
    }
    let Some((trigger, step)) = found else {
        let wrong = triggers.iter().find_map(|t| {
            crate::quests::wants_another_count(s, world, *t, given.quantity, clock.unix)
        });
        if let Some(quest) = wrong {
            let line = crate::talk::at_wrong_count(s, target_template, Some(i), quest, template);
            if let Some(line) = line {
                let speaker = crate::talk::content_speaker(s, i);
                crate::talk::say(s, net, world, &speaker, &line.words, line.how);
            }
            hand_back(s, net, world, i, &m, object, None, clock);
            return;
        }
        match crate::offerings::given(s, i, template) {
            crate::offerings::Offered::Takes => {
                crate::offerings::takes(s, net, world, i, given, clock);
                return;
            }
            crate::offerings::Offered::Refuses => {
                refused(s, net, world, i, &m, object, clock);
                return;
            }
            crate::offerings::Offered::Not => {}
        }
        match crate::plants::given(s, net, world, i, given, clock) {
            Some(true) => return,
            Some(false) => {
                hand_back(s, net, world, i, &m, object, None, clock);
                return;
            }
            None => {}
        }
        if let Some(row) =
            crate::quests::give_for(s, world, template, target_template, given.quantity)
        {
            crate::wanting::met(s, i, template);
            takes(s, net, world, i, &row, given, clock);
            return;
        }
        if crate::puzzles::given(s, m.target, template, clock) {
            hand_back(s, net, world, i, &m, object, None, clock);
            return;
        }
        match crate::map_trips::sends(s, i, template) {
            crate::map_trips::Sends::To(to) => {
                crate::map_trips::send(s, net, world, i, &m, to, clock);
            }
            crate::map_trips::Sends::Refuses => {
                crate::map_trips::refuse(s, net, world, i, &m, clock);
            }
            crate::map_trips::Sends::Not => refused(s, net, world, i, &m, object, clock),
        }
        return;
    };
    if let Some(t) = step.transforms.clone() {
        transforms(s, net, world, &step, &t, given, clock);
        return;
    }
    let speaker = crate::talk::content_speaker(s, i);
    let kept = crate::quests::triggered_by_give(
        s,
        net,
        world,
        trigger,
        Some((speaker.id, speaker.name)),
        given,
        clock,
    );
    // Steps that kept nothing hand the item back.
    if !kept {
        hand_back(s, net, world, i, &m, object, None, clock);
    }
}

/// A give that moves no quest: what the person plays and says, the item kept, what it gives in its
/// place made from the person, its effects on the player, cast by the person (2017: 1 of 1).
fn takes<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    i: usize,
    row: &yalain_content::quests::Give,
    given: Given,
    clock: Clock,
) {
    let who = s.content.objects[i].template;
    let speaker = crate::talk::content_speaker(s, i);
    if let Some(fx) = row.fx {
        let played = ServerMessage::DoFx(ObjectEvent {
            sender: crate::places::object_stamp(s, speaker.id),
            action: Fx { fx, scalar: 1.0 },
        });
        crate::places::to_knowers(s, net, speaker.id, &played);
    }
    crate::tallies::give_ran(s, row, clock);
    for line in crate::talk::with_give(s, who, Some(i), given.template) {
        crate::talk::say(s, net, world, &speaker, &line.words, line.how);
    }
    keep(s, net, world, given);
    for (n, item) in row.gives.iter().enumerate() {
        let count = row.counts.get(n).copied().unwrap_or(1).max(1);
        crate::inventory::give_from(s, net, world, *item, count, given.target);
    }
    // One more item, of several, picked by weight.
    let pick = s.item_rows.give_pick(row).map(<[(u32, u32)]>::to_vec);
    let picked = pick.and_then(|p| crate::makes::pick_one(s, &p));
    if let Some(item) = picked {
        crate::inventory::give_from(s, net, world, item, 1, given.target);
    }
    let mut told = Vec::new();
    for e in &row.effects {
        let cast = crate::effects::Cast {
            caster: speaker.id,
            spellcraft: 1.0,
            giver: Some((speaker.id, speaker.name)),
            quiet: row.effects_quiet,
            ..crate::effects::Cast::default()
        };
        told.extend(crate::effects::recipe_line(&s.effect_table, *e, cast));
        crate::effects::apply(s, net, world, *e, cast, clock);
    }
    // A text one of its effects told already is not told again.
    if let Some(text) = row.text.filter(|t| !told.contains(t)) {
        progress::private(net, world, &crate::chat::show_text(2, progress::text(text)));
    }
    for item in row.gives.iter().chain(&picked) {
        crate::quests::took(s, net, world, *item, clock);
    }
}

/// No step for the item: the person says its refusal of a give, and hands the item back.
fn refused<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    i: usize,
    m: &Move,
    object: bool,
    clock: Clock,
) {
    let template = s.content.objects[i].template;
    let item = s.things.get(m.item).map_or(0, |t| t.template);
    if let Some(line) = crate::talk::at_give(s, template, Some(i), item) {
        let speaker = crate::talk::content_speaker(s, i);
        crate::talk::say(s, net, world, &speaker, &line.words, line.how);
    }
    hand_back(s, net, world, i, m, object, None, clock);
}

/// The person hands the item back: an object at once; a person plays its handing-over and the
/// item is back when it is through. With a place, the giver is sent there as the item is back.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hand_back<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    i: usize,
    m: &Move,
    object: bool,
    to: Option<StoredPosition>,
    clock: Clock,
) {
    if object {
        back_and_away(s, net, world, m, to, clock);
        return;
    }
    let look = crate::inventory::hand_look(s, m.item);
    let id = presence::content_id(&s.content.objects, i);
    s.next_context = s.next_context.wrapping_add(1);
    let handing = handing_behavior(m.item, look, s.next_context);
    let behave = ServerMessage::PlayBehavior(ObjectEvent {
        sender: crate::places::object_stamp(s, id),
        action: handing,
    });
    crate::places::to_knowers(s, net, id, &behave);
    s.after(
        clock,
        HANDED_BACK_AFTER,
        Timer::HandedBack {
            world,
            asked: Box::new(m.clone()),
            to,
        },
    );
}

/// The item is back in the giver's pack, and the giver sent where the person sends it, if
/// anywhere.
pub fn back_and_away<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    m: &Move,
    to: Option<StoredPosition>,
    clock: Clock,
) {
    if back(s, net, world, m) {
        if let Some(to) = to {
            crate::places::sent_away(s, net, world, to, clock);
        }
    }
}

/// The item is back in the giver's pack, where it was: its container told as the person's, then
/// none, then the player's, then the move from the person; a part of a stack, which never left,
/// merged back with its container untouched (2017: 2 of 2; by the order sent, a returned item's
/// container before the answer 10 of 10). Returns whether it was.
pub fn back<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, m: &Move) -> bool {
    let Some(p) = s.players.get(&world) else {
        return false;
    };
    let me = p.character;
    let Some(slot) = p.play.items.iter().find(|h| h.id == m.item).map(|h| h.slot) else {
        return false;
    };
    let Some(t) = s.things.get(m.item) else {
        return false;
    };
    let template = t.template;
    let (look, appearance) = crate::inventory::hand_look(s, m.item);
    let part = m.quantity < t.quantity.max(1);
    let back = Move {
        move_type: GIVE,
        item: m.item,
        split_item: if part { 0 } else { m.item },
        merge: part,
        quantity: m.quantity,
        from: m.target,
        actual_from: m.target,
        target: me,
        actual_target: me,
        merge_container: me,
        actual_target_slot: i32::try_from(slot).unwrap_or(0),
        from_slot: -1,
        ignore_attunement: true,
        no_move: true,
        contain: true,
        grab: true,
        auto_merge: true,
        all_taken: true,
        played_anim: true,
        template,
        look_entity: look,
        item_appearance: appearance,
        ..Move::default()
    };
    if !part {
        for holder in [m.target, 0, me] {
            let held = ServerMessage::UpdateInstanceIdVisual(VisualQuality {
                object: crate::inventory::visual(s, m.item),
                quality: q::CONTAINER,
                value: holder,
            });
            crate::inventory::to_holder(s, net, world, m.item, &held);
        }
    }
    progress::private(net, world, &move_done(&back));
    true
}

/// The person keeps an item given: it leaves the giver's pack (a part of a stack leaves the rest,
/// its quantity told) and the world, destroyed in the person.
pub fn keep<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, given: Given) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(i) = p.play.items.iter().position(|h| h.id == given.item) else {
        return;
    };
    let left = p.play.items[i]
        .quantity
        .max(1)
        .saturating_sub(given.quantity);
    if left > 0 {
        p.play.items[i].quantity = left;
        if let Some(t) = s.things.get_mut(given.item) {
            t.quantity = left;
        }
        let m = ServerMessage::UpdateIntVisual(VisualQuality {
            object: crate::inventory::visual(s, given.item),
            quality: crate::craft::QUANTITY,
            value: i32::try_from(left).unwrap_or(i32::MAX),
        });
        crate::inventory::to_holder(s, net, world, given.item, &m);
        crate::world::keep_player(s, world);
        return;
    }
    let Ok(gone) = crate::holding::destroy_held(s, world, given.item) else {
        return;
    };
    let instance = gone.map_or(0, |t| t.instance);
    if let Some(p) = s.players.get_mut(&world) {
        crate::inventory::close_up(&mut p.play.items);
        p.known.remove(&given.item);
    }
    let gone = ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
        id: given.item,
        instance_stamp: instance,
        other_stamp: 0,
    }));
    net.send(world, queue::EVENT, BlobKind::UNORDERED, gone.encode());
    let destroyed = ServerMessage::VisualObjectCall(ObjectCall {
        object: crate::places::object_stamp(s, given.target),
        call: ClientCall {
            function: FunctionId(DESTROY_CONTAINED),
            args: Some(
                ScriptArgs::from_values(&[Arg::LongInt(given.item), Arg::LongInt(given.target)])
                    .expect("packs"),
            ),
        },
    });
    crate::places::to_knowers(s, net, given.target, &destroyed);
    crate::world::keep_player(s, world);
}

/// A step that makes the player a hero: the gift kept, the transformation's appearances and
/// behavior played on the player, and the step run when the transformation is through.
fn transforms<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    step: &Step,
    t: &Transformation,
    given: Given,
    clock: Clock,
) {
    keep(s, net, world, given);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let me = p.character;
    let mut events = Vec::new();
    for &fx in &t.fx {
        events.push(ServerMessage::DoFx(crate::places::player_event(
            p,
            Fx { fx, scalar: 1.0 },
        )));
    }
    if let Some(b) = t.behavior {
        events.push(ServerMessage::PlayBehavior(crate::places::player_event(
            p,
            BehaviorParams::behavior(b),
        )));
    }
    for m in &events {
        crate::combat::to_all_seeing(s, net, me, m);
    }
    // The gift is already kept: the step takes nothing more.
    let due = Step {
        takes: step
            .takes
            .iter()
            .copied()
            .filter(|t| *t != given.template)
            .collect(),
        ..step.clone()
    };
    s.after(
        clock,
        t.seconds,
        Timer::GiveStep {
            world,
            step: Box::new(due),
        },
    );
}

/// A player leaves the world with a give's step still to come: its gift is gone, so the step runs now.
pub(crate) fn left<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let mut due = Vec::new();
    s.timers.retain(|(_, t)| match t {
        Timer::GiveStep { world: w, step } if *w == world => {
            due.push(step.clone());
            false
        }
        _ => true,
    });
    let clock = Clock {
        local: s.saves.now,
        unix: s.saves.unix,
    };
    for step in due {
        step_due(s, net, world, &step, clock);
    }
}

/// A give's step due after its wait: run if it still may.
pub fn step_due<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    step: &Step,
    clock: Clock,
) {
    if crate::quests::may_run_now(s, world, step, clock.unix) {
        crate::quests::run(s, net, world, step, None, clock);
    }
}

/// A step's transformation is through: its player is made a hero and told the transformation's
/// text, its name in the text's variable.
pub fn made_hero<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    t: &Transformation,
) {
    let Some(name) = s.players.get(&world).map(|p| p.name.clone()) else {
        return;
    };
    if let Err(e) = crate::focus::set_hero(s, net, &name, true) {
        s.unhandled.note(format!("a hero's transformation: {e}"));
    }
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let Some(text) = t.text else {
        return;
    };
    let mut said = progress::text(text);
    if t.name != 0 {
        let literal = crate::player::world_name(&p.name, p.sex);
        said.set_variable(progress::nested(
            t.name,
            auberean_script::native_object::TextInfo {
                has_literal: 1,
                literal: Some(literal.encode_utf16().collect()),
                ..Default::default()
            },
        ));
    }
    progress::say(net, world, said);
}
