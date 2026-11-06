//! Items used on objects their template's interaction table names: a hoard's key on its stockpile, a
//! carrot on a rabbit, a bell on a dillo.
//!
//! **What an item may be used on.** An item's template may name an interaction table (its
//! `Usage_ItemInteractionTable`): for each kind of object it may be used on, how near the object the
//! player must stand, whether the use plays a behavior, and its outcome (whether the item is used
//! up, and the text the use is answered with).
//!
//! **A use at once.** An interaction that plays no behavior is carried out at once, with no walk
//! (2017: answered 0.2 to 0.5 s after the request, 33 of 33 made uses of the bell). The player
//! farther from the object than the table allows is refused, the item kept (10056, 18 of 18 bell
//! uses refused, each farther than its 8 m by the answer's own distance; made uses up to 8.6 m);
//! a player whose quest is not where the item needs it is refused as any held item is. Otherwise
//! the quest steps the use triggers run, the item is used up when the outcome says so (before the
//! answer, 11 of 13), and the use is answered: status 0, the use's control flags 319 when the item
//! went and 316 when it stayed (13 of 13 and 134 of 134), the object an item was used on as the
//! target's kind, the outcome's own text, or "You successfully use {the} %1 on {the} %2." with the
//! object's and the item's names (149 of 149 made uses whose outcome names no text).
//!
//! **A use that plays a behavior.** An interaction plays the item's own use behavior (its
//! template's `Usage_UserAnimation`), else the object's (a table naming a behavior of its own goes
//! on as any other use: none of those was recorded used on an object): a watering
//! can, a fertilizer bag or a pollen on a dye plant plays the plant's, the Requiescat on a weapon
//! its own (2017: some behavior on the table, the item or the object, 107 of 107 slow uses; none,
//! 61 of 61 at once). Refused as a use at once is, it is otherwise begun with no walk: the user
//! plays the behavior, for everyone in view (no target, no cloned look); when it has played out
//! (1.88 s for a plant's, 2.42 s for the Requiescat's: 2017's medians from behavior to answer over
//! 104 and 3 uses) the quest steps run, an item kept wears (its durability falls by one, 101 of
//! 101 cans and bags), an item used up goes one off its stack or out of the pack, and the use is
//! answered as one at once is (316 kept, 101 of 101; 319 used up, 6 of 6). While it plays any
//! other use is refused as one made while the player's actions are locked (7 of 7).
//!
//! **What it may be used on.** A table names its objects by template, or by class: a class is a
//! script package, and an object of any template whose package is it or derives from it is one
//! (the cans, bags and pollens name the dye plants' class, the Requiescat the weapons'). An object
//! the user holds is used on where the user stands.

use std::collections::HashMap;

use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use auberean_script::graph::{Graph, PackedObject};
use auberean_script::library::Library;
use auberean_script::native_object::{Item, NativeObject};
use yalain_content::quests::Trigger;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::places::{self, Use};
use crate::start::Nav;
use crate::{presence, progress, Clock, Server, Timer};

/// The property naming an item template's interaction table.
pub const TABLE: &str = "Usage_ItemInteractionTable";

/// The answer's control flags: the item used up, or kept.
pub const FLAGS_USED_UP: u32 = 319;
pub const FLAGS_KEPT: u32 = 316;

/// The kind of target an item used on an object names in its answer.
pub const ON_AN_OBJECT: u32 = 4;

/// The refusal of a player too far from the object.
pub const TOO_FAR: u32 = 10_056;

/// The answer to an interaction whose outcome names no text: "You successfully use {the} %1 on
/// {the} %2.", with the object's name and the item's.
pub mod text {
    pub const USED_ON: (u32, u32) = (0x4FB3_6953, 0x2500_0014);
    pub const TARGET: u32 = 0x1C06_9478;
    pub const ITEM: u32 = 0x9352_2CD0;
}

/// One kind of use an item's table allows.
#[derive(Debug, Clone, PartialEq)]
pub struct Interaction {
    /// The templates of the objects it may be used on (those of the classes it names among them).
    pub targets: Vec<u32>,
    /// How near the object the player must stand, in meters.
    pub reach: f32,
    /// The behavior the table itself plays, if it names one.
    pub behavior: Option<u32>,
    /// The outcome's text, if it names one.
    pub message: Option<(u32, u32)>,
    /// The outcome uses the item up.
    pub uses_up: bool,
}

/// Every interaction table the item templates name, by its saved state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Interactions {
    pub tables: HashMap<u32, Vec<Interaction>>,
}

/// The outcome's flags: the item it was used with is destroyed.
const DESTROYS_ITEM: u32 = 1;

impl Interactions {
    /// The tables of these saved states, read from their graphs (a table that does not read is
    /// left out); a class a table names counts every template of `templates` (each with its script
    /// package) whose package is it or derives from it.
    pub fn read(
        lib: &Library,
        states: &[u32],
        graph: &dyn Fn(u32) -> Option<Graph>,
        templates: &[(u32, u32)],
    ) -> Self {
        let mut out = Self::default();
        for &state in states {
            if let Some(list) = graph(state).and_then(|g| table_of(lib, &g, templates)) {
                out.tables.insert(state, list);
            }
        }
        out
    }

    /// The interaction an item of a template (its properties given) has with objects of another.
    #[must_use]
    pub fn between(&self, props: &yalain_content::Props, target: u32) -> Option<&Interaction> {
        self.tables
            .get(&props.word(TABLE)?)?
            .iter()
            .find(|i| i.targets.contains(&target))
    }
}

fn table_of(lib: &Library, g: &Graph, templates: &[(u32, u32)]) -> Option<Vec<Interaction>> {
    let nav = Nav::new(lib, g);
    let root = nav.object(g.root()?).ok()?;
    let words = |ti: &PackedObject, field: &str| -> Vec<u32> {
        nav.field(ti, field)
            .ok()
            .and_then(|r| nav.native(r).ok())
            .map_or_else(Vec::new, |n| match n {
                NativeObject::HashSet(h) => h
                    .buckets
                    .iter()
                    .flatten()
                    .filter_map(|i| match i {
                        Item::Word(w) => Some(*w),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            })
    };
    let mut out = Vec::new();
    for ti in nav.list(root, "m_listTargetInteractions").ok()? {
        let mut targets = words(ti, "m_hashValidTargets");
        let classes = words(ti, "m_hashValidTargetClasses");
        if !classes.is_empty() {
            for &(template, package) in templates {
                if !targets.contains(&template)
                    && classes.iter().any(|c| lib.derives_from(package, *c))
                {
                    targets.push(template);
                }
            }
        }
        let outcome = nav
            .list(ti, "m_listSuccessOutcomes")
            .ok()
            .and_then(|l| l.first().copied());
        let message = outcome
            .and_then(|o| nav.field(o, "m_siMessage").ok())
            .and_then(|r| nav.native(r).ok())
            .and_then(|n| match n {
                NativeObject::TextInfo(t) if t.string_id != 0 => Some((t.string_id, t.table)),
                _ => None,
            });
        let flags = outcome
            .and_then(|o| nav.field(o, "m_uiFlags").ok())
            .unwrap_or(0);
        out.push(Interaction {
            targets,
            reach: nav
                .number(ti, "m_fExternalTargetMaxDistance")
                .unwrap_or(0.0),
            behavior: nav.field(ti, "m_anim").ok().filter(|a| *a != 0),
            message,
            uses_up: flags & DESTROYS_ITEM != 0,
        });
    }
    Some(out)
}

/// The template, place and name (a string reference) of an object an item is used on.
type Target = (u32, StoredPosition, Option<(u32, u32)>);

/// The template, place and name of the object an item is used on: an object of the world, a
/// thing the world made, or one the user holds (where the user stands).
fn target_of<S: Store>(s: &Server<S>, world: ConnId, id: u64) -> Option<Target> {
    if let Some(i) = presence::content_by_id(&s.content.objects, id) {
        let o = &s.content.objects[i];
        let at = StoredPosition {
            cell: o.cell,
            origin: o.origin,
            rotation: o.rotation,
        };
        return Some((o.template, at, o.description.name));
    }
    let t = s.things.get(id)?;
    let at = match t.at {
        Some(at) => at,
        None => {
            let p = s.players.get(&world)?;
            held_by(s, id, p.character).then_some(p.position)?
        }
    };
    Some((t.template, at, t.facts.name))
}

/// Whether a player holds a thing: in its pack (or a container there) or wielded.
fn held_by<S: Store>(s: &Server<S>, id: u64, player: u64) -> bool {
    let mut at = id;
    for _ in 0..8 {
        let Some(t) = s.things.get(at) else {
            return false;
        };
        let Some((holder, _)) = t.container.or(t.wielded) else {
            return false;
        };
        if holder == player {
            return true;
        }
        at = holder;
    }
    false
}

/// The behavior a use plays: the table's, else the item's use behavior, else the object's.
fn behavior_of<S: Store>(s: &Server<S>, how: &Interaction, item: u32, target: u32) -> Option<u32> {
    let own = |t: u32| {
        s.catalog
            .get(t)
            .and_then(|t| t.props.word(USER_BEHAVIOR))
            .filter(|b| *b != 0)
    };
    how.behavior.or_else(|| own(item)).or_else(|| own(target))
}

/// The template property naming a use's behavior.
pub const USER_BEHAVIOR: &str = "Usage_UserAnimation";

/// The bits of an interaction's behavior: the behavior, its hold cycles (none), the words of flags
/// `0x20` and `0x100` (each 1), and the context.
pub const BEHAVIOR_BITS: u32 =
    BehaviorParams::BEHAVIOR | BehaviorParams::HOLD_CYCLES | 0x20 | 0x100 | BehaviorParams::CONTEXT;

/// The dye plants' behavior and the Requiescat's.
pub const PLANT_BEHAVIOR: u32 = 0x4100_0020;
pub const REQUIESCAT_BEHAVIOR: u32 = 0x4100_00FC;

/// How long a behavior plays before the use is answered, in seconds (2017's medians from the
/// behavior to the answer; another behavior as an item's use takes).
#[must_use]
pub fn behavior_seconds(behavior: u32) -> f64 {
    match behavior {
        PLANT_BEHAVIOR => 1.88,
        REQUIESCAT_BEHAVIOR => 2.42,
        other => crate::item_use::behavior_seconds(other),
    }
}

/// A held item used on an object its table names: refused or carried out at once, or, when it
/// plays a behavior, begun. False when the item's table has no such interaction (the use goes on
/// as any other).
pub fn used<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    clock: Clock,
) -> bool {
    let Some(item) = s.things.get(u.item).map(|t| t.template) else {
        return false;
    };
    let Some((target, at, _)) = target_of(s, world, u.target) else {
        return false;
    };
    let Some(props) = s.catalog.get(item).map(|t| t.props.clone()) else {
        return false;
    };
    let Some(how) = s.interactions.between(&props, target).cloned() else {
        return false;
    };
    // A step that takes the player somewhere plays the use and its trip instead; a table that
    // plays a behavior of its own (no 2017 use of one on an object was recorded) goes on as any
    // other use.
    let trigger = Trigger::UseOn(item, target);
    if how.behavior.is_some()
        || s.quests
            .steps
            .iter()
            .any(|st| st.trigger == trigger && st.to.is_some())
    {
        return false;
    }
    let Some(p) = s.players.get(&world) else {
        return true;
    };
    let distance = places::reach(&p.position, at.cell, at.origin).unwrap_or(f32::MAX);
    if distance > how.reach {
        let m = places::use_answer_on(u, FLAGS_KEPT, distance, TOO_FAR);
        progress::private(net, world, &m);
        return true;
    }
    if let Some(why) = crate::item_use::quest_gate(s, world, &props, clock) {
        let m = places::use_answer_on(u, FLAGS_KEPT, distance, why);
        progress::private(net, world, &m);
        return true;
    }
    let Some(behavior) = behavior_of(s, &how, item, target) else {
        carry_out(s, net, world, u, false, clock);
        return true;
    };
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return true;
    };
    let mut b = BehaviorParams {
        bits: BEHAVIOR_BITS,
        behavior,
        context,
        ..BehaviorParams::default()
    };
    // The words of flags 0x20 and 0x100 (the second and the seventh of the flags with a word).
    b.words[1] = 1;
    b.words[6] = 1;
    let plays = ServerMessage::PlayBehavior(places::player_event(p, b));
    let id = p.character;
    places::to_all_knowing(s, net, id, &plays);
    s.after(
        clock,
        behavior_seconds(behavior),
        Timer::Interaction {
            world,
            used: Box::new(u.clone()),
        },
    );
    true
}

/// An interaction's behavior has played out: it is carried out, if the item and the object are
/// still there and the table still names it.
pub fn played<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    clock: Clock,
) {
    // The item may have left the pack while the behavior played.
    let held = s
        .players
        .get(&world)
        .is_some_and(|p| p.play.items.iter().any(|h| h.id == u.item));
    let Some(item) = s.things.get(u.item).map(|t| t.template).filter(|_| held) else {
        return;
    };
    let Some((target, _, _)) = target_of(s, world, u.target) else {
        return;
    };
    let Some(props) = s.catalog.get(item).map(|t| &t.props) else {
        return;
    };
    if s.interactions.between(props, target).is_none() {
        return;
    }
    carry_out(s, net, world, u, true, clock);
}

/// Carry an interaction out: its quest steps, the item worn (when its behavior played and it is
/// kept) or used up, the answer.
fn carry_out<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    played: bool,
    clock: Clock,
) {
    let Some(item) = s.things.get(u.item).map(|t| t.template) else {
        return;
    };
    let Some((target, at, target_name)) = target_of(s, world, u.target) else {
        return;
    };
    let Some(how) = s
        .catalog
        .get(item)
        .and_then(|t| s.interactions.between(&t.props, target))
        .cloned()
    else {
        return;
    };
    let distance = s
        .players
        .get(&world)
        .and_then(|p| places::reach(&p.position, at.cell, at.origin))
        .unwrap_or(0.0);
    let trigger = Trigger::UseOn(item, target);
    crate::quests::triggered(s, net, world, trigger, None, clock);
    let item_name = s.things.get(u.item).and_then(|t| t.facts.name);
    if how.uses_up {
        crate::craft::use_up(s, net, world, u.item, 1);
    } else if played {
        crate::butchery::wear_tool_by(s, net, world, u.item, 1.0, 0);
    }
    let mut told = u.clone();
    told.message = Some(match how.message {
        Some(m) => progress::text(m),
        None => {
            let vars = [
                target_name.map(|n| progress::nested(text::TARGET, progress::text(n))),
                item_name.map(|n| progress::nested(text::ITEM, progress::text(n))),
            ]
            .into_iter()
            .flatten()
            .collect();
            progress::text_with(text::USED_ON, vars)
        }
    });
    let flags = if how.uses_up {
        FLAGS_USED_UP
    } else {
        FLAGS_KEPT
    };
    let m = places::use_answer_on(&told, flags, distance, 0);
    progress::private(net, world, &m);
    // A dye plant cared for or pollinated.
    crate::plants::cared(s, net, world, (item, u.target), clock);
}
