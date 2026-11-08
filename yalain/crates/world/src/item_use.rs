//! Items used from the pack for the effects their templates name: potions, elixirs, cards, charms,
//! the Crystalline Tyrant Eggs and the like.
//!
//! **What an item does.** Its template names up to eight effects (`Usage_Effect1`...), each with
//! the spellcraft it is cast at (`Usage_EffectN_Spellcraft`, else 1). A name is a value of the
//! game's effect enumeration ([`enum_of`]), and the effect put on is the saved state of that value
//! among the effects'; a name the enumeration lacks that is a package of the script library puts
//! on the package's one saved state, else its singleton.
//!
//! **Who may use it.** Before anything is done a use is refused, its answer alone, when its item
//! may not be used moving and the player moves; when it mounts its user and the player's hands
//! hold anything, a mount is already on, or the player is indoors; when it must be used near a place and the player is
//! farther off (with the item's own word on it); when it needs a quest and the player's is not
//! where it needs it; when the player is below its least level.
//!
//! **A use that takes time.** An item with a use behavior (its template's `Usage_UserAnimation`,
//! else the drink for a potion's usage action) is used from the pack at once: the player plays the
//! behavior with the item cloned into its hand, for everyone in view; when it has played out (a
//! drink 4.24 s, the others about 2.8 s) everything happens in one instant: the item's effects
//! put on its user in order, cast by it from the item, the item spent, the use answered with
//! "You drink {the} %1." (the drink) or "You use {the} %1." (the others), or the template's own
//! message. Nothing else may be done while it plays; a jump or the user's death cuts it short.
//!
//! **A use at once.** An item with no use behavior does the same at once, its answer keeping the
//! request's message.
//!
//! **Spent.** An item destroyed on use (its template's flag, or its kind's) goes one off its
//! stack, or out of the pack when it is the last; any other is kept (the eggs, the charms that
//! are not). The answer's control flags say so.
//!
//! **An effect that lasts until taken off** (an egg's mount, which has no duration) is a record
//! that never runs out and goes at logout.
//!
//! **An item that is only looked at** (its usage action the examination's, no effects) is shown
//! to the player as an examination, then the use answered at once.

use std::collections::HashMap;

use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use yalain_content::Props;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::effect_defs::Kind;
use crate::effects::{self, Cast, On, Shape};
use crate::places::Use;
use crate::usable::maps;
use crate::{progress, Clock, Server, Timer};

/// One effect an item's use puts on its user: the saved state and the spellcraft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UsageEffect {
    pub state: u32,
    pub spellcraft: f32,
}

/// The effects each item template's use puts on, by template.
pub type ItemUses = HashMap<u32, Vec<UsageEffect>>;

/// The effect enumeration's value of each effect name item templates give their uses.
const ENUMS: &str = include_str!("effect_enums.tsv");

/// The effect enumeration's value of an effect's name, when the game names it.
#[must_use]
pub fn enum_of(name: &str) -> Option<u32> {
    static TABLE: std::sync::OnceLock<HashMap<&'static str, u32>> = std::sync::OnceLock::new();
    TABLE
        .get_or_init(|| {
            ENUMS
                .lines()
                .filter(|l| !l.starts_with('#'))
                .filter_map(|l| {
                    let (n, v) = l.split_once('\t')?;
                    Some((n, v.trim().parse().ok()?))
                })
                .collect()
        })
        .get(name)
        .copied()
}

/// The saved state an effect's name puts on: the state of its enumeration value (`of_enum`), else
/// its package's one saved state (`own` of the package), else the package's singleton.
pub fn resolve(
    name: &str,
    of_enum: impl Fn(u32) -> Option<u32>,
    package_of: impl Fn(&str) -> Option<u32>,
    own: impl Fn(u32) -> Option<u32>,
) -> Option<u32> {
    if let Some(s) = enum_of(name).and_then(of_enum) {
        return Some(s);
    }
    let p = package_of(name)?;
    Some(own(p).unwrap_or(0x7000_0000 | p))
}

/// The effects a template's properties name for its use, in order, resolved by `resolve` (a name
/// that resolves to nothing is left out).
pub fn usage_effects(props: &Props, resolve: impl Fn(&str) -> Option<u32>) -> Vec<UsageEffect> {
    named_effects(props, USED, resolve)
}

/// The properties naming an item's effects and their spellcraft (`{name}{n}`, `{name}{n}{after}`):
/// on its user, and on whoever wears or wields it.
pub const USED: (&str, &str) = ("Usage_Effect", "_Spellcraft");
pub const WORN: (&str, &str) = ("Item_EquipperEffect", "Stat");
/// And on whoever kills a creature.
pub const DIED: (&str, &str) = ("NPC_DeathEffect", "Stat");

/// The effects a template's properties name by one pair of [`USED`] and [`WORN`], in order, each at
/// its spellcraft (else 1), resolved by `resolve` (a name that resolves to nothing is left out).
pub fn named_effects(
    props: &Props,
    (name, after): (&str, &str),
    resolve: impl Fn(&str) -> Option<u32>,
) -> Vec<UsageEffect> {
    (1..=8)
        .filter_map(|i| {
            let n = props.text(&format!("{name}{i}"))?;
            Some(UsageEffect {
                state: resolve(n)?,
                spellcraft: props.float(&format!("{name}{i}{after}")).unwrap_or(1.0),
            })
        })
        .collect()
}

/// The effects of these templates' uses, the effect states known by their package and their
/// enumeration value (`states`: each saved state among the effects', its package and its value).
pub fn read<'a>(
    lib: &auberean_script::library::Library,
    states: impl IntoIterator<Item = (u32, u32, Option<u32>)>,
    templates: impl IntoIterator<Item = (u32, &'a Props)>,
) -> ItemUses {
    read_named(lib, states, templates, USED)
}

/// The effects these templates name by one pair of [`USED`] and [`WORN`], resolved as [`read`]
/// resolves them.
pub fn read_named<'a>(
    lib: &auberean_script::library::Library,
    states: impl IntoIterator<Item = (u32, u32, Option<u32>)>,
    templates: impl IntoIterator<Item = (u32, &'a Props)>,
    props: (&str, &str),
) -> ItemUses {
    let mut of_package: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut of_value: HashMap<u32, Vec<u32>> = HashMap::new();
    for (state, package, value) in states {
        of_package.entry(package).or_default().push(state);
        if let Some(v) = value {
            of_value.entry(v).or_default().push(state);
        }
    }
    let one = |m: &HashMap<u32, Vec<u32>>, k: u32| match m.get(&k).map(Vec::as_slice) {
        Some([one]) => Some(*one),
        _ => None,
    };
    templates
        .into_iter()
        .filter_map(|(id, p)| {
            let list = named_effects(p, props, |n| {
                resolve(
                    n,
                    |v| one(&of_value, v),
                    |x| lib.package_id(x),
                    |p| one(&of_package, p),
                )
            });
            (!list.is_empty()).then_some((id, list))
        })
        .collect()
}

/// A forager's food (a Piece of Food): its template names no effect for its use, but its use heals
/// health and vigor at the food's spellcraft, the forage's (2017: 40 for 10 of the 15 foods used,
/// their heals 244 each, 50 of 66 vital updates; 45 and 19 for the others), and wears it by one;
/// at its last durability it is destroyed through overuse (12 of 12, at the fifth use).
pub const FOOD: u32 = 0x4700_0845;
pub const FOOD_EFFECTS: [u32; 2] = [0x6F00_2255, 0x6F00_2251];
pub const FOOD_SPELLCRAFT: f32 = 40.0;
pub const FOOD_DURABILITY: u32 = 5;

/// The training hall's potions (haste, healing, thorns): their templates name no effect, but each
/// is drunk for its one effect, at the spellcraft of 1 (2017: 5 of 5 drinks, 2 haste, 2 healing,
/// 1 thorns).
pub const TRAINING_POTIONS: [(u32, u32); 3] = [
    (0x4700_1AEE, 0x7000_0A13),
    (0x4700_1AEF, 0x7000_0A00),
    (0x4700_1AF0, 0x7000_09FD),
];

/// The effect a training hall's potion is drunk for, if the template is one.
fn training_potion(template: u32) -> Option<u32> {
    TRAINING_POTIONS
        .iter()
        .find(|(t, _)| *t == template)
        .map(|(_, state)| *state)
}

/// The effects an item's use puts on that the server's own code names, not its template.
#[must_use]
pub fn own_effects(template: u32) -> Vec<UsageEffect> {
    if template == FOOD {
        FOOD_EFFECTS
            .iter()
            .map(|&state| UsageEffect {
                state,
                spellcraft: FOOD_SPELLCRAFT,
            })
            .collect()
    } else if let Some(state) = training_potion(template) {
        vec![UsageEffect {
            state,
            spellcraft: 1.0,
        }]
    } else {
        Vec::new()
    }
}

/// The drink a potion is used with, and the potions' usage action.
pub const DRINK: u32 = 0x4100_0023;
pub const POTION_ACTION: u32 = 0x2B;

/// The usage action of the eggs' mounts (their answer's flags differ).
pub const MOUNT_ACTION: u32 = 0x30;

/// The usage action of an item that is only looked at.
pub const EXAMINE_ACTION: u32 = 0x22;

/// Why a use is refused, as its answer says.
pub mod refusal {
    /// The player is below the item's least level.
    pub const LEVEL_TOO_LOW: u32 = 10_009;
    /// The player is not of the kingdom the object needs.
    pub const WRONG_KINGDOM: u32 = 10_011;
    /// The player has never had the quest the item needs.
    pub const LACKING_QUEST: u32 = 10_019;
    /// The player's quest is not where the item needs it.
    pub const WRONG_QUEST_STATUS: u32 = 10_020;
    /// "Only monarchs can use that item."
    pub const MONARCH_ONLY: u32 = 10_038;
    /// A take: the player's quest is not where the item needs it.
    pub const WRONG_QUEST_STATUS_TAKE: u32 = 10_021;
    /// A take: the player has never had the quest the item needs.
    pub const LACKING_QUEST_TAKE: u32 = 10_022;
    /// A take whose hidden quest is not where the item needs it.
    pub const MYSTERIOUS_TAKE: u32 = 10_042;
    /// A use whose hidden quest is not where the item or object needs it.
    pub const MYSTERIOUS_USE: u32 = 10_043;
    /// "Only a Hero of Dereth can use that item."
    pub const HERO_ONLY: u32 = 10_049;
    /// A mount's item, with something in the player's hands.
    pub const HANDS_NOT_EMPTY: u32 = 10_045;
    /// A mount's item, with a mount already on.
    pub const MOUNTED: u32 = 10_047;
    /// A mount's item, indoors.
    pub const INDOORS: u32 = 10_046;
    /// Farther from the item's place than its radius.
    pub const NOT_THERE: u32 = 10_054;
    /// An item that may not be used moving, while the player moves.
    pub const MOVING: u32 = 10_055;
}

/// The hands' places among the places items are worn.
pub const HANDS: u32 = 0x40 | 0x80;

/// The ways an item tells a player far from its place where it is: a direction, how far.
pub mod feedback {
    pub const DIRECTION: u32 = 1;
    pub const DISTANCE: u32 = 2;
}

/// How long a use behavior plays before the use is carried out, in seconds (2017: the drink a
/// median 4.24 s, 129 uses; `0x410000FC` 2.77 s, 47; a seed's planting 2.0 s, 9).
#[must_use]
pub fn behavior_seconds(behavior: u32) -> f64 {
    if behavior == DRINK {
        4.24
    } else if behavior == crate::plants::PLANTING_BEHAVIOR {
        crate::plants::PLANTING_SECONDS
    } else {
        2.77
    }
}

/// The bits of a use behavior: the behavior, the item it is played toward, its look cloned, flag
/// `0x100` and the context.
pub const BEHAVIOR_BITS: u32 = BehaviorParams::BEHAVIOR
    | 0x100
    | BehaviorParams::TARGET
    | BehaviorParams::CLONE
    | BehaviorParams::CONTEXT;

/// The answer's control flags.
pub mod flags {
    /// A use that took its time.
    pub const PLAYED: u32 = 316;
    /// A use at once.
    pub const AT_ONCE: u32 = 444;
    /// An egg's mount.
    pub const MOUNT: u32 = 828;
    /// The item was spent.
    pub const SPENT: u32 = 0x1;
    /// The use holds its user still while it plays (a Tessera's does not).
    pub const LOCKS: u32 = 0x8;
    /// The template asks for its user to be let go for its effects.
    pub const UNLOCKS: u32 = 0x2000;
    /// The item may not be used moving.
    pub const NOT_WHILE_MOVING: u32 = 0x200;
    /// What a use at once carried out adds (a refusal never carries it).
    pub const CARRIED_OUT: u32 = 0x80;
}

/// The answers' texts, and the item's variable in them.
pub mod text {
    pub const DRINK: (u32, u32) = (0xF6C3_0A14, 0x2500_0014);
    pub const USE: (u32, u32) = (0x9D44_2979, 0x2500_0014);
    pub const ITEM: u32 = 0x9352_2CD0;
}

/// The flags of a record that lasts until taken off: it never runs out, it goes at logout.
pub const UNTIL_TAKEN_OFF: u32 =
    effects::record_flag::INFINITE | effects::record_flag::LOGOUT | effects::record_flag::TRANSIENT;

/// An item's use under way: the request, and the behavior it plays.
#[derive(Debug, Clone, PartialEq)]
pub struct Using {
    pub used: Use,
    pub behavior: u32,
}

/// The use behavior of an item's template: its own, else the drink for a potion's usage action.
#[must_use]
pub fn use_behavior(props: &Props) -> Option<u32> {
    props
        .word("Usage_UserAnimation")
        .or_else(|| (props.word("Item_UsageAction") == Some(POTION_ACTION)).then_some(DRINK))
}

/// The use behavior of an item of a template: its template's, else the drink for a forager's food
/// and a training hall's potion (2017: the drink played, 65 of 65 uses; 5 of 5).
#[must_use]
pub fn behavior_of(template: u32, props: &Props) -> Option<u32> {
    use_behavior(props)
        .or((template == FOOD || training_potion(template).is_some()).then_some(DRINK))
}

/// A Tessera's use behavior. Its use plays it, and at its end gives its user a hero skill credit
/// more (the skill repository sent with it), spends the Tessera and is answered "You use {the}
/// %1." with flags that neither lock the user nor stop it moving (2017: 10 of 10 uses, 2.6 s after
/// the request, flags 309; the user moved while it played).
pub const TESSERA_BEHAVIOR: u32 = 0x4100_00FC;

/// The use behavior of one item: a treasure potion's (one with an effect of its own) the drink, a
/// Tessera's its own, else its template's ([`behavior_of`]).
#[must_use]
pub fn item_behavior<S: Store>(
    s: &Server<S>,
    item: u64,
    template: u32,
    props: &Props,
) -> Option<u32> {
    let t = s.things.get(item);
    if t.is_some_and(|t| t.own.effect.is_some()) {
        Some(DRINK)
    } else if t.is_some_and(|t| s.tesserae.contains(&t.description.package)) {
        Some(TESSERA_BEHAVIOR)
    } else {
        behavior_of(template, props)
    }
}

/// A player uses an item it holds that names effects for its use, or is only looked at: it is
/// refused, begins (or, with no use behavior, is carried out). Returns false for any other item.
pub fn begin<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    clock: Clock,
) -> bool {
    let Some(template) = s.things.get(u.item).map(|t| t.template) else {
        return false;
    };
    if crate::craft::totem::noun(template).is_some() {
        crate::craft::use_totem(s, net, world, u);
        return true;
    }
    let Some(t) = s.catalog.get(template) else {
        return false;
    };
    let examined = t.props.word("Item_UsageAction") == Some(EXAMINE_ACTION);
    let seed = crate::plants::is_seed(template);
    let own = s
        .things
        .get(u.item)
        .is_some_and(|i| i.own.effect.is_some() || s.tesserae.contains(&i.description.package));
    if s.item_uses.get(&template).is_none_or(Vec::is_empty)
        && own_effects(template).is_empty()
        && !examined
        && !seed
        && !own
        && !crate::death_spawn::names_one(s, &t.props)
    {
        return false;
    }
    let look = t.description.look;
    let props = t.props.clone();
    if refuse(s, net, world, u, &props, clock) {
        return true;
    }
    if cannot_make(s, template, &props) {
        s.unhandled.note(format!(
            "a use of {template:08X}, which makes what can't be made"
        ));
        let me = s.players.get(&world).map_or(0, |p| p.character);
        let flags = answer_flags(&props, use_behavior(&props).is_some()) & !flags::CARRIED_OUT;
        progress::private(
            net,
            world,
            &crate::chat::show_text(1, progress::text(crate::pets::text::SUMMONING_FAILS)),
        );
        let no = crate::places::use_refused(u, me, Some(flags), 0.0, crate::places::USE_STATUS_OK);
        progress::private(net, world, &no);
        return true;
    }
    let Some(behavior) = item_behavior(s, u.item, template, &props) else {
        carry_out(s, net, world, u.clone(), false, clock);
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
        target: u.item,
        clone_visual: look,
        context,
        ..BehaviorParams::default()
    };
    // The word of flag 0x100 (the seventh of the flags followed by a word).
    b.words[6] = 1;
    let plays = ServerMessage::PlayBehavior(crate::places::player_event(p, b));
    let id = p.character;
    crate::places::to_all_knowing(s, net, id, &plays);
    s.after(
        clock,
        behavior_seconds(behavior),
        Timer::ItemUse {
            world,
            using: Box::new(Using {
                used: u.clone(),
                behavior,
            }),
        },
    );
    true
}

/// The effects of items whose use makes something no rule here makes (a band, an explosion), though
/// they read as changing nothing rather than as a pet.
const NOT_MADE: [&str; 2] = ["Eff_Tsys_Item_Summon_", "Eff_DestrucfluxMassive_"];

/// Would using an item of this template make what can't be made? A seed whose plant isn't described,
/// a row's creature that isn't, or an effect that summons what no item's use makes. The item is then
/// kept: nothing is used up for nothing.
fn cannot_make<S: Store>(s: &Server<S>, template: u32, props: &Props) -> bool {
    if let Some(species) = crate::plants::sown_by(template) {
        return !crate::makes::can_make(s, species.plant);
    }
    if crate::death_spawn::names_one(s, props) {
        return !crate::death_spawn::can_make_all(s, props);
    }
    let names_unmade = (1..=8)
        .filter_map(|i| props.text(&format!("{}{i}", USED.0)))
        .any(|e| NOT_MADE.iter().any(|n| e.starts_with(n)));
    names_unmade
        || s.item_uses.get(&template).is_some_and(|list| {
            list.iter().any(|e| {
                matches!(
                    s.effect_table.get(e.state).map(|d| &d.kind),
                    Some(Kind::Pet { .. })
                )
            })
        })
}

/// A use refused, if the player may not use an item of these properties now: answered with its
/// status alone (and, far from the item's place, told where it is). Returns whether it was.
pub(crate) fn refuse<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    props: &Props,
    clock: Clock,
) -> bool {
    let Some(why) = refused(s, world, props, clock) else {
        return false;
    };
    if why == refusal::NOT_THERE {
        tell_where(s, net, world, props);
    }
    let me = s.players.get(&world).map_or(0, |p| p.character);
    let flags = answer_flags(props, use_behavior(props).is_some()) & !flags::CARRIED_OUT;
    let no = crate::places::use_refused(u, me, Some(flags), 0.0, why);
    progress::private(net, world, &no);
    true
}

/// Is this for monarchs alone, and the player none? A monarch is the head of an allegiance; a
/// player in none is not one.
#[must_use]
pub fn not_monarch<S: Store>(s: &Server<S>, world: ConnId, props: &Props) -> bool {
    props.flag("Usage_MonarchOnly")
        && s.players
            .get(&world)
            .is_none_or(|p| s.allegiances.monarch(p.character) != Some(p.character))
}

/// Why a player may not use an item of these properties now, if it may not: for monarchs alone and
/// the player none, moving (an item that may not be used so), a mount's item with its hands full,
/// a mount on or indoors, too far from the item's place, its quest (one the server knows) not where
/// the item needs it, below the item's least level.
fn refused<S: Store>(s: &Server<S>, world: ConnId, props: &Props, clock: Clock) -> Option<u32> {
    let p = s.players.get(&world)?;
    if not_monarch(s, world, props) {
        return Some(refusal::MONARCH_ONLY);
    }
    if not_while_moving(props) && crate::skill_use::moving(p) {
        return Some(refusal::MOVING);
    }
    if props.word("Item_UsageAction") == Some(MOUNT_ACTION) {
        if p.play.items.iter().any(|h| h.location & HANDS != 0) {
            return Some(refusal::HANDS_NOT_EMPTY);
        }
        if mounted(s, p) {
            return Some(refusal::MOUNTED);
        }
        if crate::presence::indoors(p.position.cell) {
            return Some(refusal::INDOORS);
        }
    }
    if let Some((cell, origin)) = props.place("Usage_RequiredLocation") {
        let place = StoredPosition {
            cell,
            origin,
            rotation: [1.0, 0.0, 0.0, 0.0],
        };
        let radius = props.float("Usage_RequiredLocationRadius").unwrap_or(0.0);
        if maps::ground_distance(&p.position, &place) > radius {
            return Some(refusal::NOT_THERE);
        }
    }
    if let Some(why) = quest_gate(s, world, props, clock) {
        return Some(why);
    }
    let least = props.int("Usage_MinLevel").unwrap_or(0);
    if i64::from(crate::examine::level_of(&p.qualities)) < least {
        return Some(refusal::LEVEL_TOO_LOW);
    }
    None
}

/// May an item of these properties not be used moving?
fn not_while_moving(props: &Props) -> bool {
    matches!(
        props.get("Item_CanUseWhileMoving"),
        Some(yalain_content::Prop::Bool(false))
    )
}

/// Is a mount on the player: it rides, or a record an item that mounts its user put on is on it?
fn mounted<S: Store>(s: &Server<S>, p: &crate::world::Player) -> bool {
    crate::mounts::mounted(p)
        || p.play.effects.iter().any(|e| {
            e.item != 0
                && s.things
                    .get(e.item)
                    .and_then(|t| s.catalog.get(t.template))
                    .is_some_and(|t| t.props.word("Item_UsageAction") == Some(MOUNT_ACTION))
        })
}

/// Why a quest is not where an item needs it, if it is not: needed under way (at a phase, when
/// the status names one) or completed, the player never had it (10019) or has it otherwise
/// (10020); needed had in any way, the player never had it (10019) or cancelled it (10020); needed
/// able to be given, the player has it under way or completed and not yet to be taken again (10020).
#[must_use]
pub fn quest_refusal(
    held: &[crate::state::QuestState],
    quest: u32,
    needed: u32,
    retry: f64,
    now_unix: i64,
) -> Option<u32> {
    use crate::quests::status;
    let had = held.iter().find(|q| q.quest == quest);
    let raw = needed & 0xF000_0000;
    if raw == CAN_BE_BESTOWED {
        let again = had.is_none_or(|q| {
            #[allow(clippy::cast_precision_loss)]
            let waited = (now_unix - q.updated) as f64;
            q.status != status::UNDERWAY
                && (q.status != status::COMPLETED || (retry > 0.0 && waited >= retry))
        });
        return (!again).then_some(refusal::WRONG_QUEST_STATUS);
    }
    let Some(q) = had else {
        return Some(refusal::LACKING_QUEST);
    };
    if needed == yalain_content::QuestGate::HAD {
        let had = [status::UNDERWAY, status::FAILED, status::COMPLETED].contains(&q.status);
        return (!had).then_some(refusal::WRONG_QUEST_STATUS);
    }
    let phase = needed & 0x0FFF_FFFF;
    let fits = q.status == raw && (raw != status::UNDERWAY || phase == 0 || q.phase == phase);
    (!fits).then_some(refusal::WRONG_QUEST_STATUS)
}

/// Why the quest a use of something of these properties needs keeps a player out, if it does:
/// its `Usage_RequiredQuest` at its `Usage_RequiredQuestStatus` (completed when it names none), as
/// [`quest_refusal`] judges it; a hidden quest's refusal is the mysterious one (2017: 8 of 8
/// refusals for a hidden quest, 13 of 13 for a shown one with the lacking or wrong-status word). A
/// quest the server holds no definition of is never had, and is not asked for.
pub fn quest_gate<S: Store>(
    s: &Server<S>,
    world: ConnId,
    props: &Props,
    clock: Clock,
) -> Option<u32> {
    let quest = props.word("Usage_RequiredQuest")?;
    let needed = props
        .word("Usage_RequiredQuestStatus")
        .unwrap_or(crate::quests::status::COMPLETED);
    needs_quest(s, world, quest, needed, clock)
}

/// Why a quest a use needs at a status (written as `Usage_RequiredQuestStatus` is) keeps a player
/// out, if it does, as [`quest_gate`] judges a template's.
pub fn needs_quest<S: Store>(
    s: &Server<S>,
    world: ConnId,
    quest: u32,
    needed: u32,
    clock: Clock,
) -> Option<u32> {
    let def = s.quests.quest(quest)?;
    let held = &s.players.get(&world)?.play.quests;
    let why = quest_refusal(held, def.id, needed, def.retry, clock.unix)?;
    Some(if def.hidden {
        refusal::MYSTERIOUS_USE
    } else {
        why
    })
}

/// Why the quest taking an item of these properties needs keeps a player out, if it does: its
/// `Usage_RequiredQuest_ForTaking` at its `Usage_RequiredQuestStatus_ForTaking` (under way, at any
/// phase, when it names none: 2017's takes of such items by players with the quest under way went
/// through, 6 of 6); never had: 10022, had otherwise: 10021 (2017: 4 of 4 and 2 of 2); a hidden
/// quest's refusal the mysterious one.
pub fn take_gate<S: Store>(
    s: &Server<S>,
    world: ConnId,
    props: &Props,
    clock: Clock,
) -> Option<u32> {
    let def = s
        .quests
        .quest(props.word("Usage_RequiredQuest_ForTaking")?)?;
    let held = &s.players.get(&world)?.play.quests;
    let needed = props
        .word("Usage_RequiredQuestStatus_ForTaking")
        .unwrap_or(crate::quests::status::UNDERWAY);
    let why = quest_refusal(held, def.id, needed, def.retry, clock.unix)?;
    Some(if def.hidden {
        refusal::MYSTERIOUS_TAKE
    } else if why == refusal::LACKING_QUEST {
        refusal::LACKING_QUEST_TAKE
    } else {
        refusal::WRONG_QUEST_STATUS_TAKE
    })
}

/// The quest status an item needs when it needs its quest able to be given.
pub const CAN_BE_BESTOWED: u32 = 0x5000_0000;

/// Tell a player too far from an item's place how far it is, as the item tells it: an item that
/// tells the distance says which of its bands the player is in, or that the player is outside
/// them all.
fn tell_where<S: Store>(s: &Server<S>, net: &mut ServerNet, world: ConnId, props: &Props) {
    if props.word("Usage_RequiredLocationFeedbackType") != Some(feedback::DISTANCE) {
        return;
    }
    let (Some(p), Some((cell, origin)), Some(table)) = (
        s.players.get(&world),
        props.place("Usage_RequiredLocation"),
        props.word("Usage_ErrorMessagesTableID"),
    ) else {
        return;
    };
    let place = StoredPosition {
        cell,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let band = |k: &str| props.float(k).unwrap_or(0.0);
    let bands = [
        band("Usage_RequiredLocationCloseProximityMsgRadius"),
        band("Usage_RequiredLocationMediumProximityMsgRadius"),
        band("Usage_RequiredLocationFarProximityMsgRadius"),
        band("Usage_RequiredLocationVeryFarProximityMsgRadius"),
    ];
    let d = maps::ground_distance(&p.position, &place);
    let sid = maps::band_of(&bands, d).map_or(maps::text::OUTSIDE, |b| maps::text::BANDS[b].0);
    progress::private(
        net,
        world,
        &crate::chat::show_text(maps::SHOWN_AS, progress::text((sid, table))),
    );
}

/// An item's use behavior has played out: the use is carried out.
pub fn played<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    using: &Using,
    clock: Clock,
) {
    carry_out(s, net, world, using.used.clone(), true, clock);
}

/// The answer's status of a use cut short.
pub const INTERRUPTED: u32 = 91;

/// A use whose behavior is playing is cut short (its user jumped or died): answered as interrupted
/// with the flags of a use that takes its time, nothing done, the item kept.
pub fn cut_short<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(i) = s
        .timers
        .iter()
        .position(|(_, t)| matches!(t, Timer::ItemUse { world: w, .. } if *w == world))
    else {
        return;
    };
    let (_, timer) = s.timers.remove(i);
    let Timer::ItemUse { using, .. } = timer else {
        return;
    };
    let me = s.players.get(&world).map_or(0, |p| p.character);
    let no = crate::places::use_refused(&using.used, me, Some(flags::PLAYED), 0.0, INTERRUPTED);
    progress::private(net, world, &no);
}

/// Carry an item's use out, in one instant: its effects on its user, the item spent, the answer.
fn carry_out<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    mut u: Use,
    played: bool,
    clock: Clock,
) {
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    // The item may have left the pack while the behavior played.
    let held = s
        .players
        .get(&world)
        .is_some_and(|p| p.play.items.iter().any(|h| h.id == u.item));
    let Some(template) = s.things.get(u.item).map(|t| t.template).filter(|_| held) else {
        return;
    };
    let Some(t) = s.catalog.get(template).cloned() else {
        return;
    };
    if t.props.word("Item_UsageAction") == Some(EXAMINE_ACTION) {
        let examine = crate::places::force_examine(u.item);
        progress::private(net, world, &examine);
    }
    let mut list = s.item_uses.get(&template).cloned().unwrap_or_default();
    list.extend(own_effects(template));
    // A treasure potion's own effect, at its own spellcraft.
    let drunk = s.things.get(u.item).and_then(|i| i.own.effect);
    list.extend(drunk.map(|e| UsageEffect {
        state: e.state,
        spellcraft: e.spellcraft,
    }));
    for e in list {
        put_on(s, net, world, me, u.item, e, clock);
    }
    // One that makes what a world's row names moves the quest it names too, before it's answered
    // (2017: a practice gem's quest 0.06 s before its answer, 4 of 4).
    if crate::death_spawn::names_one(s, &t.props) {
        for e in crate::quests::named_for_use(s, &t.props) {
            crate::quests::moves(s, net, world, e, true, clock);
        }
    }
    // A Tessera's hero skill credit.
    let behavior = item_behavior(s, u.item, template, &t.props);
    // By its kind: other items play this behavior too, and give no credit.
    let tessera = s
        .things
        .get(u.item)
        .is_some_and(|i| s.tesserae.contains(&i.description.package));
    if tessera {
        if let Some(p) = s.players.get_mut(&world) {
            p.play.hero_credits = p.play.hero_credits.saturating_add(1);
            let m = crate::skills::repository_update(p.play.credits, p.play.hero_credits);
            progress::private(net, world, &m);
        }
    }
    // A seed's plant stands.
    crate::plants::sow(s, net, world, template, clock);
    let spent = spends(s, &t);
    if spent {
        crate::craft::use_up(s, net, world, u.item, 1);
    }
    let mut f = answer_flags(&t.props, played);
    if tessera {
        f &= !flags::LOCKS;
    }
    if spent {
        f |= flags::SPENT;
    }
    let item_name = t.description.name.map(progress::text);
    let own = match t.props.get("Usage_SuccessMessage") {
        Some(yalain_content::Prop::Str(sid, table)) => Some((*sid, *table)),
        _ => None,
    };
    let said = own.or_else(|| {
        played.then_some(if behavior == Some(DRINK) {
            text::DRINK
        } else {
            text::USE
        })
    });
    if let Some(id) = said {
        let vars = item_name
            .map(|n| vec![progress::nested(text::ITEM, n)])
            .unwrap_or_default();
        u.message = Some(progress::text_with(id, vars));
    }
    let done = crate::places::use_done(&u, me, f, false, 0.0);
    progress::private(net, world, &done);
    crate::death_spawn::used(s, net, world, &t.props, clock);
    // Food wears with each use, and goes at its last.
    if template == FOOD {
        crate::butchery::wear_tool_by(s, net, world, u.item, 1.0, FOOD_DURABILITY);
    }
    crate::world::keep_player(s, world);
}

/// The control flags of a use's answer that did not spend its item: a use that took its time,
/// a mount, or a use at once; the item may not be used moving; it lets its user go for its effects.
fn answer_flags(props: &Props, played: bool) -> u32 {
    let mut f = if played {
        flags::PLAYED
    } else if props.word("Item_UsageAction") == Some(MOUNT_ACTION) {
        flags::MOUNT
    } else {
        flags::AT_ONCE
    };
    if not_while_moving(props) {
        f |= flags::NOT_WHILE_MOVING;
    }
    if props.flag("Usage_ShouldUnlockUserForUsageEffects") {
        f |= flags::UNLOCKS;
    }
    f
}

/// Is an item of this template spent by its use: destroyed on use by its own flag or its kind's?
fn spends<S: Store>(s: &Server<S>, t: &yalain_content::Template) -> bool {
    t.props.flag("Item_IsDestroyOnUse") || s.destroyed_on_use.contains(&t.description.package)
}

/// Put one of an item's effects on its user, cast by it from the item: one with no duration that
/// changes qualities lasts until taken off; any other as its kind does it.
pub(crate) fn put_on<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    me: u64,
    item: u64,
    e: UsageEffect,
    clock: Clock,
) {
    let Some(def) = s.effect_table.get(e.state).cloned() else {
        s.unhandled.note(format!(
            "an item's effect {:08X} the server does not know",
            e.state
        ));
        return;
    };
    let cast = Cast {
        caster: me,
        spellcraft: e.spellcraft,
        pk: false,
        skill: 0,
        item,
        giver: None,
        answers: false,
    };
    match def.kind {
        // What it does is the server's own (an antidote's cure, a portal): not made here.
        Kind::Other
        | Kind::Immunity { .. }
        | Kind::Pet { .. }
        | Kind::Aura(_)
        | Kind::Recall(_)
        | Kind::Counter { .. }
        | Kind::Reply { .. }
        | Kind::Watcher(_)
        | Kind::Ward { .. } => {
            s.unhandled
                .note(format!("an item's effect {:08X} of no kind made", e.state));
        }
        Kind::Numeric { .. } | Kind::Chained(_) if def.duration.is_empty() => {
            let shape = Shape {
                flags: UNTIL_TAKEN_OFF,
                categories: None,
                timeout: Some(f32::MAX),
                acting: 0,
            };
            effects::add_on(s, net, On::Player(world), &def, cast, shape, clock);
        }
        _ => {
            effects::apply(s, net, world, e.state, cast, clock);
        }
    }
}
