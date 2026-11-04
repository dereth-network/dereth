//! Skills used by name: a player's special attacks and its recalls. The client asks with the
//! object it has selected, the skill and a number of its own for the use; the server carries the
//! use out or refuses it.
//!
//! **Refused**, in one tick: the player attacking (unless it already was), the use over (its
//! number), why (the client writes the reason), and not attacking again. The checks, first failure
//! first: dead; not wielding what the skill is used with; no vigor at all; the skill still waiting
//! (its own recovery since its last use, or the wait every skill shares after a recall); vigor
//! short of the cost; an effect the skill needs its user to carry missing; the target gone or dead,
//! too far or too close ([`crate::reach`]: the distance between the bodies, beyond the skill's
//! greatest range, within its least); moving, for a skill not used on the move.
//!
//! **A helpful skill** (a heal, a buff) is aimed at the player the request names, else at its
//! user. It plays as a special does, its story the skill's behavior on its user or on another;
//! at the skill's hook its effects reach the target ([`crate::effects::apply`]), cast at the
//! user's rating in the skill. The effects a skill leaves on its user (a combination's link to its
//! next skill) are put on as the use ends.
//!
//! **A hostile skill** used on a creature rolls how each of its hooks comes out on each target it
//! reaches: a hook with an area reaches every live creature whose body is within its radius of its
//! center's (the target, or the user for a skill used on its user; 2017: 1,653 of 1,776 creatures
//! within the radius named, 62 of 1,730 beyond it), each with its own outcome and damage, named in
//! the story one after another; any other hook reaches the target alone (a missile's area too:
//! 1,290 of 1,292 missiles of area skills hit their target alone). Where a hook lands, the skill's
//! target effects are put on that creature ([`crate::effects::apply_on`]): a debuff, a change over
//! time, a combination's link, or a combination that finds one (a creature's opening is one such:
//! an effect it takes on as it attacks).
//!
//! **A hostile skill aimed at another player** is refused, with the reason, where who may harm
//! whom forbids it ([`crate::kingdoms::may_harm`]); else it is used as on a creature (2017: every
//! special at a player, in 2017's two fights, told the story the skill tells at creatures), the
//! player reached by its hooks as the creature would be, each hook that hurts it giving the health
//! lost as PK damage, and its target effects put on that player.
//!
//! **A toggled skill** (one whose user effect is an aura) turns its aura on when its use ends: the
//! aura's first pulse and record, not attacking, the toggle on, the skill's information marked
//! toggled, the pulse's appearance, peace. Used again while on, it turns off at once: attacking,
//! the use over, the toggle off, the aura's records taken off, "You stop …", not attacking, the
//! appearance stopped.
//!
//! **Cut short**: moving cuts a use not used on the move; stopping all attacks cuts it, and
//! stopping attacks on its target cuts one aimed at it. After its story it counts as used.
//!
//! **An attack special** from rest: the player attacking, in its weapon's combat mode, turned to
//! its target (a skill used from afar) or walked to it (one used within reach); then its vigor
//! paid (the skill's cost and its weapon's) and the story of the special: the skill's behavior at
//! the player's attack time scale, the use's number, the skill, and one hook per hook of the skill
//! (a missile's naming the weapon that throws it, a strike's with its result at once). A missile
//! arrives after the throw's windup and its flight, and the target tells how it came out; a strike
//! lands at its animation's hit point. The special's damage is a basic attack's with the hook's
//! own added. When the special's animation is over, the skill's information (when it was last
//! used, from which its recovery counts), the player not attacking, stopped and at peace. Inside
//! an attack run, a special takes the run's next swing, and the run goes on after it.
//!
//! **A recall** powers up for its time (the player's behavior held), then its story and its vigor
//! (the whole of it), into portal space and out where its recall effect says, its text ("You
//! recall to your lifestone."), and the end: the skill's information and the wait every skill
//! shares (30 seconds). Where it goes ([`recall_of`]): the lifestone the player bound to; the
//! last portal it used that may be recalled to (Portal Recall); a city's place (the cities'
//! recalls, which protect it for 30 s as they end); the hall its allegiance is bound to. A player
//! with no such place plays it all and is told it failed. Moving while it powers up interrupts
//! it, and nothing is spent.
//!
//! **A summoning** (a portal to the lifestone or to the last portal's place) plays as a recall
//! does to its story; then its portal appears ([`crate::usable::portals`]) and the use ends.
//!
//! **One request waits** while a use runs: a newer one replaces it (the older is over, with no
//! reason), and it is carried out, or refused, as the running one ends.

use auberean_primitives::Vec3;
use auberean_protocol::physics::{MoveOrder, ObjectEvent, Word};
use auberean_protocol::quality::PrivateQuality;
use auberean_protocol::types::{
    BehaviorParams, MoveTarget, MovementParams, PhysicsStory, Placement, StoryHook,
};
use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::native_object::TextInfo;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::calls::Value;
use crate::combat::{self, player_event, to_all_seeing};
use crate::effect_defs::{EffectDef, Kind as EffectKind, Recall};
use crate::player::quality;
use crate::rules::{self, Kind, Outcome};
use crate::skills::{Hook, Usage};
use crate::{progress, quals, session, Clock, Server, Timer};

/// The calls of a skill's use.
pub mod function {
    /// The client's request: use a skill on a target.
    pub const USE: u32 = 0x8204_000B;
    /// A use is over (its number).
    pub const ENDED: u32 = 0x8204_0019;
    /// A use was refused: why, the target, the skill.
    pub const REFUSED: u32 = 0x8204_001A;
}

/// Why a use is refused or cut short (the client writes the reason).
pub mod refusal {
    pub const INTERRUPTED: u32 = 91;
    pub const MOVE_INTERRUPTED: u32 = 54;
    pub const NOT_ENOUGH_VIGOR: u32 = 20_000;
    pub const TARGET_GONE: u32 = 20_002;
    pub const TARGET_DEAD: u32 = 22_000;
    pub const NO_VIGOR: u32 = 30_016;
    pub const TOO_CLOSE: u32 = 30_018;
    pub const TOO_FAR: u32 = 30_019;
    pub const WRONG_IMPLEMENT: u32 = 30_026;
    pub const MOVING: u32 = 30_027;
    pub const NOT_RESET: u32 = 30_030;
    pub const MISSING_EFFECT: u32 = 30_024;
    pub const DEAD: u32 = 30_041;
}

/// The player's qualities of the wait every skill shares: how long it lasts, and when it last
/// began (game time).
pub mod q {
    pub const RESET_DURATION: u32 = 3000;
    pub const LAST_RESET: u32 = 303;
}

/// A used skill's flags (its definition's interface flags).
pub mod ui_flag {
    pub const USE_WHILE_MOVING: u32 = 0x1;
    pub const IGNORE_WEAPON_VIGOR: u32 = 0x20;
    pub const IGNORE_SHIELD_VIGOR: u32 = 0x80;
    pub const NON_DAMAGING: u32 = 0x200;
    /// The player is walked to its target (rather than refused as too far).
    pub const MOVE_TO: u32 = 0x800;
    pub const SELF_TARGETED: u32 = 0x2000;
    pub const IGNORE_RESET: u32 = 0x4_0000;
}

/// A hook's flags.
pub mod hook_flag {
    pub const AREA: u32 = 0x1;
    pub const PROJECTILE: u32 = 0x4;
    pub const NON_DAMAGING: u32 = 0x40;
    /// The hand that strikes: which weapon's damage the blow does.
    pub const LEFT_HAND: u32 = 0x1000;
}

/// A skill information's mask: the skill has a time it was last used; a toggled skill is on
/// (2017: 0xA3 at every aura turned on).
pub const HAS_LAST_USED: u32 = 0x20;
pub const TOGGLED_ON: u32 = 0x80;

/// The recalls: lifestone, last portal, the cities', the allegiance's.
pub const LIFESTONE_RECALL: u32 = 835;
pub const PORTAL_RECALL: u32 = 870;
pub const CRAGSTONE_RECALL: u32 = 925;
pub const ALLEGIANCE_RECALL: u32 = 999;
pub const ARRAMORA_RECALL: u32 = 1045;

/// Texts: a recall's, in its recall effect's own table ("You recall to your lifestone.", "You
/// fail to recall to your lifestone."); fatigue's (string id, table).
pub mod text {
    pub const RECALLED: u32 = 0x912E_FF1D;
    pub const RECALL_FAILED: u32 = 0xCF46_E5A6;
    pub const FATIGUED: (u32, u32) = (0xF902_C5FE, 0x2500_000B);
}

/// From the beginning to a special's story, with no power-up (2017: a median 0.134 s).
pub const STORY_AFTER: f64 = 0.134;
/// A power-up's length over its definition's time (2017's humans: a recall's 10 s power-up 12.68 s
/// to its story, Inner Calm's 1 s 1.257 s).
pub const POWERUP_SCALE: f64 = 1.268;
/// From a recall's story into portal space, in portal space, and out of it to the use's end
/// (2017's humans: 2.48, 1.52, 0.18 s); a recall that fails says so 0.70 s after its story and ends
/// 0.60 s later.
pub const RECALL_TO_PORTAL: f64 = 2.48;
pub const RECALL_IN_PORTAL: f64 = 1.52;
pub const RECALL_ENDS_AFTER: f64 = 0.18;
pub const RECALL_FAILS_AFTER: f64 = 0.70;
pub const FAILED_ENDS_AFTER: f64 = 0.60;
/// A special's animation from its story to its end, at time scale 1, by behavior (2017: Deadeye's
/// 1.878, Riposte's 1.444, Mind Lash's behavior 2.080, the heals' 1.43-1.51, Blinding Arrow's and
/// Sinew Snap's 3.25: 3.124 s to what came next at time scale 1.053, 46, and 1.373 s at 2.64, 147);
/// others 1.6.
#[must_use]
pub fn animation_seconds(behavior: u32) -> f64 {
    match behavior {
        0x4100_007A => 1.878,
        0x4100_007B => 3.25,
        0x4100_0077 => 1.444,
        0x4100_00B9 => 2.080,
        0x4100_00C2 | 0x4100_00C9 => 1.47,
        _ => 1.6,
    }
}

/// When a helpful skill's effects reach their target after its story, at time scale 1, by
/// behavior and whether it is used on its user (2017: Ease Suffering's 0.667 on its user, 0.722 on
/// another; Inner Calm's 0.477; a timed buff's about 0.97).
#[must_use]
pub fn hook_seconds(behavior: u32, on_self: bool) -> f64 {
    match behavior {
        0x4100_00C2 if on_self => 0.667,
        0x4100_00C2 => 0.722,
        0x4100_00C9 => 0.477,
        _ => 0.97,
    }
}
/// A strike lands at this share of its animation, over the time scale (2017: Riposte's hits
/// 0.70-0.72 s x time scale after the story).
pub const STRIKE_LANDS: f64 = 0.71;
/// A missile's windup before it flies, in seconds (2017: Deadeye's about 0.98 s).
pub const MISSILE_WINDUP: f64 = 0.98;
/// After a special inside a run, the run's next swing, in seconds (2017: a median 0.123 s).
pub const NEXT_SWING_AFTER: f64 = 0.123;
/// A recall's wait every skill shares, in seconds, unless the definition says otherwise.
pub const RESET_SECONDS: f32 = 30.0;

/// The behavior bits of a special's story: the behavior, the time scale, and the words of flags
/// 0x400 (1) and 0x200 (0); with flag 0x20's word (1) for behaviors that carry it (Deadeye's, Mind
/// Lash's, the recall's: 2017 sent it for those always).
pub const STORY_BEHAVIOR_BITS: u32 = 0x605;
/// The behaviors whose stories carry flag 0x20's word: the specials', the heals', and the basic
/// attacks of bows from afar and of staves.
pub const WITH_WORD_20: [u32; 13] = [
    0x4100_007A,
    0x4100_00B9,
    0x4100_00D5,
    0x4100_00C9,
    0x4100_00C2,
    0x4100_00E2,
    0x4100_00E3,
    0x4100_00E4,
    0x4100_00E5,
    0x4100_00E6,
    0x4100_00B0,
    0x4100_00B1,
    0x4100_00B2,
];

/// A special's story bits: the attacker, its behavior, the use's number, the skill.
pub const SPECIAL_STORY: u32 =
    PhysicsStory::ATTACKER | PhysicsStory::BEHAVIOR | PhysicsStory::CONTEXT | PhysicsStory::SKILL;
/// A missile's arrival, told by its target: the attacker and the skill.
pub const ARRIVAL_STORY: u32 = PhysicsStory::ATTACKER | PhysicsStory::SKILL;

/// Hooks of a special's story.
pub mod story_hook {
    /// Always first: kind 2, nothing more.
    pub const OPENING: u32 = 0x2;
    /// A hook that only plays: kind 0 with the target and its result.
    pub const PLAYS: u32 = 0x180;
    /// A strike: kind 1 with the target, result, animation hook, and the health change on a hit.
    pub const STRIKE_HIT: u32 = 0xB81;
    pub const STRIKE_MISSED: u32 = 0x381;
    /// A missile: kind 3 with the target, the weapon throwing it, the animation hook.
    pub const MISSILE: u32 = 0x323;
    /// The bit of a hook after the first: its index follows.
    pub const ATTACK_HOOK: u32 = 0x400;
}

/// Implement kinds (bits a skill's definition allows).
pub mod implement {
    /// An empty hand, and what holds nothing a skill asks of a hand (a ring, a tool, a lute).
    pub const NONE: u32 = 0x1;
    pub const SWORD: u32 = 0x2;
    pub const BOW: u32 = 0x4;
    pub const DRUM: u32 = 0x8;
    pub const SHIELD: u32 = 0x10;
    pub const AXE: u32 = 0x20;
    pub const DAGGER: u32 = 0x40;
    pub const GUNBLADE: u32 = 0x80;
    pub const HAMMER: u32 = 0x100;
    pub const HANDBLADE: u32 = 0x200;
    pub const POLEARM: u32 = 0x400;
    pub const ALCHEMY: u32 = 0x800;
    pub const BOULDER: u32 = 0x1000;
    pub const WASP_NEST: u32 = 0x2000;
    pub const WRENCH: u32 = 0x4000;
    pub const SPEAR: u32 = 0x8000;
    pub const CESTAS: u32 = 0x1_0000;
    pub const FLAIL: u32 = 0x2_0000;
    pub const STAVE: u32 = 0x4_0000;
    pub const SCYTHE: u32 = 0x8_0000;
    pub const HAND: u32 = 0x10_0000;
    pub const ORB: u32 = 0x20_0000;
}

/// The script classes that say what their items are wielded as, and what they say. No class here
/// derives from another.
pub const IMPLEMENT_CLASSES: [(&str, u32); 31] = [
    ("MiningToolTemplate", implement::NONE),
    ("ToolTemplate", implement::NONE),
    ("RingTemplate", implement::NONE),
    ("TalismanTemplate", implement::NONE),
    ("LuteTemplate", implement::NONE),
    ("EarringsToadfang", implement::NONE),
    ("ShieldTemplate", implement::SHIELD),
    ("AlchemyTemplate", implement::ALCHEMY),
    ("AxeTemplate", implement::AXE),
    ("BoulderTemplate", implement::BOULDER),
    ("BowTemplate", implement::BOW),
    ("CestasTemplate", implement::CESTAS),
    ("DaggerTemplate", implement::DAGGER),
    ("DrumTemplate", implement::DRUM),
    ("FlailTemplate", implement::FLAIL),
    ("FrogstaveTemplate", implement::STAVE),
    ("HammerTemplate", implement::HAMMER),
    ("HandbladeTemplate", implement::HANDBLADE),
    ("HandTemplate", implement::HAND),
    ("GunbladeTemplate", implement::GUNBLADE),
    ("OrbTemplate", implement::ORB),
    ("JailaiTemplate", implement::CESTAS),
    ("PolearmTemplate", implement::POLEARM),
    ("ScepterLugianTemplate", implement::STAVE),
    ("ScytheTemplate", implement::SCYTHE),
    ("SharpstickTemplate", implement::AXE),
    ("SpearTemplate", implement::SPEAR),
    ("StaveTemplate", implement::STAVE),
    ("SwordTemplate", implement::SWORD),
    ("WaspnestTemplate", implement::WASP_NEST),
    ("WrenchTemplate", implement::WRENCH),
];

/// The implement kinds of a catalog's templates: for each, what the class its package derives
/// from says; none for a template of no such class.
#[must_use]
pub fn implements_of_catalog(
    catalog: &yalain_content::Catalog,
    derives_from: impl Fn(u32, &str) -> bool,
) -> std::collections::BTreeMap<u32, u32> {
    catalog
        .templates
        .values()
        .filter_map(|t| {
            IMPLEMENT_CLASSES
                .iter()
                .find(|(class, _)| derives_from(t.description.package, class))
                .map(|(_, i)| (t.id, *i))
        })
        .collect()
}

/// The starting weapons' implements (their classes'), and their vigor costs (4 each: their
/// templates name none).
pub const STARTING_WEAPONS: [(u32, u32); 9] = [
    (0x4700_051B, implement::BOW),
    (0x4700_051C, implement::SWORD),
    (0x4700_051D, implement::STAVE),
    (0x4700_0515, implement::SWORD),
    (0x4700_0516, 0x100),
    (0x4700_0517, implement::STAVE),
    (0x4700_0521, 0x8),
    (0x4700_0522, 0x8000),
    (0x4700_0523, implement::SWORD),
];
pub const STARTING_WEAPON_VIGOR: i32 = 4;

/// Combat modes by what is wielded.
pub mod mode {
    pub const BOW: u32 = 0x4100_0010;
    pub const STAVE: u32 = 0x4100_0011;
    pub const ONE_HANDED: u32 = 0x4100_000A;
    pub const WITH_SHIELD: u32 = 0x4100_000B;
    pub const POLEARM: u32 = 0x4100_000C;
    pub const TWO_WEAPONS: u32 = 0x4100_000E;
    pub const HANDBLADE: u32 = 0x4100_000F;
    pub const FLAIL: u32 = 0x4100_0013;
    pub const ALCHEMY: u32 = 0x4100_0015;
}

/// The private effect a player's client plays when it exploits an opening, and a creature's
/// opening (the golden rays) with what exploiting it does.
pub const EXPLOITED_FX: u32 = 0x4100_051A;
pub const OPENING_FX: u32 = 0x4100_0003;
pub const EXPLOIT_DAMAGE: i32 = 20;
pub const EXPLOIT_HOOK: u32 = 0x981;
/// How long a creature's opening lasts after it attacks, and when it opens, in seconds (2017:
/// 7.5 s; the rays a median 1.14 s after its attack's story).
pub const OPENING_SECONDS: f64 = 7.5;
pub const OPENS_AFTER: f64 = 1.14;
/// A creature's opening: the effect it takes on a moment after its plain and strong strikes
/// (seven and a half seconds; its type is what the specials' combinations look for).
pub const OPENING_EFFECT: u32 = 0x7000_06D7;

/// A use as the client asks for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// The object the player had selected (0: none).
    pub target: u64,
    pub skill: u32,
    /// The client's number for this use.
    pub id: u32,
}

impl Request {
    /// Read a request from its call's values (packed last parameter first: the use's number, the
    /// skill, the target).
    #[must_use]
    pub fn read(v: &[Value]) -> Option<Self> {
        match v {
            [Value::Int(id), Value::Int(skill), Value::Long(target)] => Some(Self {
                target: *target,
                skill: *skill,
                id: *id,
            }),
            _ => None,
        }
    }
}

/// The steps of a use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The story (after a power-up, or at once).
    Story,
    /// A strike lands on its target.
    Strike,
    /// A missile arrives.
    Arrives,
    /// A recall: into portal space (or its failure told).
    Portal,
    /// A recall: out of portal space.
    Out,
    /// A summoning: the portal appears.
    Summon,
    /// A helpful skill's effects reach its target.
    Hook,
    /// The use is over.
    End,
}

/// Whom a use is aimed at: the user itself, another player (its world connection and object),
/// or a thing of the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aim {
    Me,
    Player(ConnId, u64),
    Thing(u64),
}

/// A use under way.
#[derive(Debug, Clone, PartialEq)]
pub struct Using {
    pub req: Request,
    pub usage: Usage,
    /// When it began (game time), which the skill's information keeps.
    pub began: f64,
    /// It raised the attack state itself (not inside a run).
    pub alone: bool,
    /// Its story has been told (its vigor paid).
    pub told: bool,
    pub time_scale: f32,
    /// How its first damaging hook came out, and the damage.
    pub outcome: Option<(Outcome, i32)>,
    /// Whom it is aimed at, and whether the player was turned or walked toward it.
    pub aim: Aim,
    pub turned: bool,
    /// The skill's rating as it began (its effects' spellcraft).
    pub spellcraft: f32,
    /// How each of its striking hooks came out on each target it reached.
    pub landings: Vec<Landing>,
}

/// One hook of a use on one target: how it came out, and the damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Landing {
    pub target: u64,
    /// The hook's index among the skill's.
    pub hook: usize,
    pub outcome: Outcome,
    pub damage: i32,
    /// What the target's effects send back.
    pub back: i32,
}

/// A special's missile in flight: its request, how it is used, how it will come out.
#[derive(Debug, Clone, PartialEq)]
pub struct Flying {
    pub req: Request,
    pub usage: Usage,
    pub outcome: Option<(Outcome, i32)>,
    /// The skill's rating as it was used (its target effects' spellcraft).
    pub spellcraft: f32,
}

/// A creature's opening: until when (world seconds).
pub type Opening = f64;

/// The call telling the client a use is over.
#[must_use]
pub fn ended(id: u32) -> ServerMessage {
    session::call(function::ENDED, &[Arg::Int(id)]).expect("packs")
}

/// The call telling the client why a use was refused (packed last parameter first).
#[must_use]
pub fn refused(err: u32, target: u64, skill: u32) -> ServerMessage {
    session::call(
        function::REFUSED,
        &[Arg::Int(skill), Arg::LongInt(target), Arg::Int(err)],
    )
    .expect("packs")
}

/// What a player holds in its hands: each thing and its template. A thing that takes both hands
/// (a bow, a stave) is the right hand's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hands {
    pub right: Option<(u64, u32)>,
    pub left: Option<(u64, u32)>,
}

/// What a player holds in its hands (what it wears on its body is not among them).
pub(crate) fn hands<S: Store>(s: &Server<S>, player: u64) -> Hands {
    let mut hands = Hands::default();
    let Some(p) = s.players.values().find(|p| p.character == player) else {
        return hands;
    };
    for h in &p.play.items {
        if h.location == crate::equip::OFF_HAND {
            hands.left = Some((h.id, h.template));
        } else if h.location != 0 && h.location & crate::equip::BODY_PLACES == 0 {
            hands.right = Some((h.id, h.template));
        }
    }
    hands
}

/// The things a player wields, with their templates: the right hand's, then the left's.
pub(crate) fn wielded<S: Store>(s: &Server<S>, player: u64) -> Vec<(u64, u32)> {
    let hands = hands(s, player);
    hands.right.into_iter().chain(hands.left).collect()
}

/// What a starting weapon is wielded as.
#[must_use]
pub fn starting_implement(template: u32) -> Option<u32> {
    STARTING_WEAPONS
        .iter()
        .find(|(t, _)| *t == template)
        .map(|(_, i)| *i)
}

/// Is a template a bow? Its objects are made strung: their own mode is the bow's (2017: 1,273 of
/// 1,273 bows made so, 26,018 other things a hand holds at peace).
#[must_use]
pub fn is_bow(t: &yalain_content::Template) -> bool {
    starting_implement(t.id) == Some(implement::BOW)
        || t.props.word("Item_ImplementType") == Some(implement::BOW)
}

/// What a template is wielded as (its implement kind), when the server knows it.
#[must_use]
pub fn implement_of(catalog: &yalain_content::Catalog, template: u32) -> Option<u32> {
    starting_implement(template).or_else(|| catalog.implements.get(&template).copied())
}

/// The combat mode of what is wielded, by the right hand's implement and the left's (2017, the
/// mode a player was in as it swung, by the implements the skill allows; of 166,210 stories 1,519
/// came at peace and 1 otherwise: handblades 16,631, polearms 407, two one-handed weapons 6,782,
/// one with a shield 31,149, one alone 701, bows, spears, orbs and hammers 22,457, staves, drums
/// and hands 44,926, alchemy, cestas and boulders 41,582, wasp nests and flails 55).
#[must_use]
pub fn mode_of(right: Option<u32>, left: Option<u32>) -> u32 {
    use implement as i;
    match right {
        Some(i::BOW | i::SPEAR | i::ORB | i::HAMMER | i::WRENCH) => mode::BOW,
        Some(i::STAVE | i::DRUM | i::HAND) => mode::STAVE,
        Some(i::HANDBLADE) => mode::HANDBLADE,
        Some(i::POLEARM) => mode::POLEARM,
        Some(i::ALCHEMY | i::CESTAS | i::BOULDER) => mode::ALCHEMY,
        Some(i::WASP_NEST | i::FLAIL) => mode::FLAIL,
        _ => match left {
            Some(i::SHIELD) => mode::WITH_SHIELD,
            Some(i::SWORD | i::AXE | i::DAGGER | i::SCYTHE) => mode::TWO_WEAPONS,
            _ => mode::ONE_HANDED,
        },
    }
}

/// The player's combat mode: the stance its right hand's template names for what the left holds, alone,
/// with a shield or with a second weapon (2017: 11,460 of 11,460 basic swings not at peace, each kind of
/// hands among them); [`mode_of`] for a template that names none.
pub fn player_mode<S: Store>(s: &Server<S>, player: u64) -> u32 {
    let h = hands(s, player);
    let of = |hand: Option<(u64, u32)>| hand.and_then(|(_, t)| implement_of(&s.catalog, t));
    let (right, left) = (of(h.right), of(h.left));
    let quality = match (h.left, left) {
        (None, _) => "Weapon_SingleWeaponStance",
        (_, Some(implement::SHIELD)) => "Weapon_ShieldStance",
        _ => "Weapon_TwoWeaponStance",
    };
    h.right
        .and_then(|(_, t)| s.catalog.get(t))
        .and_then(|t| t.props.word(quality))
        .filter(|stance| *stance != 0)
        .unwrap_or_else(|| mode_of(right, left))
}

/// Is a skill used with what the player wields? Each hand's implement must be among the skill's
/// allowed ones for that hand (an empty hand is the implement "none", bit 1: so a skill allowing
/// all but none, the recall's, needs something in the right hand). A wielded thing whose implement
/// the server does not know is taken to be allowed.
#[must_use]
pub fn implement_allowed(catalog: &yalain_content::Catalog, u: &Usage, hands: Hands) -> bool {
    let fits = |allowed: i32, hand: Option<(u64, u32)>| {
        let allowed = allowed.cast_unsigned();
        match hand {
            None => allowed & implement::NONE != 0,
            Some((_, t)) => implement_of(catalog, t).is_none_or(|i| allowed & i != 0),
        }
    };
    fits(u.implements_right, hands.right) && fits(u.implements_left, hands.left)
}

/// What a player's hands add to a skill's vigor cost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeldVigor {
    /// Its weapons', one or two.
    pub weapons: i32,
    /// Its shield's.
    pub shield: i32,
}

/// What each thing in a player's hands costs its skills: the item's own number (a treasure or
/// made item's), else its template's; a starting weapon's 4 (2017: an item's own or its template's
/// number, both weapons' when it holds two; 237 of 248 uses by players whose costs nothing else
/// lowered paid them on themselves too). What the player's auras multiply it by comes off each item,
/// rounded down (2017: a weapon of 18 examined 13 under Balance and 11 under Featherweight, 30 at 22,
/// 16 at 11, a shield of 7 at 5, 67 profiles; uses under a Balance recorded, 77 of 84 exact).
pub fn held_vigor<S: Store>(s: &Server<S>, player: u64) -> HeldVigor {
    let mut held = HeldVigor::default();
    for (item, template) in wielded(s, player) {
        let shield = implement_of(&s.catalog, template) == Some(implement::SHIELD);
        let on_it = crate::effects::on_held(s, player, item, shield);
        let scale = crate::effects::held_multiplier(s, &on_it, crate::made::stat::VIGOR);
        let own = s
            .things
            .get(item)
            .and_then(|t| t.own.made)
            .and_then(|m| m.vigor);
        let cost = own
            .or_else(|| {
                s.catalog
                    .get(template)
                    .and_then(|t| t.props.int("Weapon_VigorCost"))
                    .and_then(|c| i32::try_from(c).ok())
            })
            .unwrap_or(if starting_implement(template).is_some() {
                STARTING_WEAPON_VIGOR
            } else {
                0
            });
        #[allow(clippy::cast_precision_loss)]
        let cost = dereth_primitives::num::to_i32((cost as f32 * scale).floor());
        if shield {
            held.shield += cost;
        } else {
            held.weapons += cost;
        }
    }
    held
}

/// A skill's vigor cost for a player with this greatest vigor and these things held: the greatest
/// vigor times the definition's share, less its added part (negative), what its weapons cost and
/// what its shield costs, unless the skill ignores them (2017, a shield of 5 beside a weapon of 5:
/// 10 for a skill that does not ignore it, 5 for one that does).
#[must_use]
pub fn vigor_cost(u: &Usage, max_vigor: i32, held: HeldVigor) -> i32 {
    vigor_cost_with(u, max_vigor, held, 0.0)
}

/// [`vigor_cost`] with what the player's effects add, a fraction rounded up, the whole at least half
/// the skill's own (2017, a buff of the stat alone on three players: its reckoning exact on 665 of 666
/// uses, 98 of 104, 63 of 63; where the reckoning went under half, 99 of 99 paid half; other players
/// paid a few less, by what wasn't seen).
#[must_use]
pub fn vigor_cost_with(u: &Usage, max_vigor: i32, held: HeldVigor, effects: f32) -> i32 {
    #[allow(clippy::cast_precision_loss)]
    let base = dereth_primitives::num::to_i32(max_vigor as f32 * u.vigor_mult)
        - dereth_primitives::num::to_i32(u.vigor_add);
    let weapons = if u.ui_flags & ui_flag::IGNORE_WEAPON_VIGOR == 0 {
        held.weapons
    } else {
        0
    };
    let shield = if u.ui_flags & ui_flag::IGNORE_SHIELD_VIGOR == 0 {
        held.shield
    } else {
        0
    };
    #[allow(clippy::cast_precision_loss)]
    let whole = dereth_primitives::num::to_i32(((base + weapons + shield) as f32 + effects).ceil());
    whole.max((base + 1) / 2)
}

/// What a skill costs this player now.
fn cost_now<S: Store>(s: &Server<S>, p: &crate::world::Player, u: &Usage, max_vigor: i32) -> i32 {
    vigor_cost_with(
        u,
        max_vigor,
        held_vigor(s, p.character),
        crate::effects::vigor_cost_added(s, p),
    )
}

/// Is the player moving, as its last report says (an asked motion, or a jump)?
pub(crate) fn moving(p: &crate::world::Player) -> bool {
    p.heading_motion & 0x00FF_FFFF != crate::world::IDLE_MOTION
}

/// A skill's recall, if it is one: where its recall effect takes its user, and the string table
/// of its texts (the effect's name's).
#[must_use]
pub fn recall_of<S: Store>(s: &Server<S>, skill: u32) -> Option<(Recall, u32)> {
    let u = s.skill_table.skills.get(&skill)?.usage.as_ref()?;
    u.target_effects
        .iter()
        .chain(&u.user_effects)
        .find_map(|e| match s.effect_table.get(*e) {
            Some(EffectDef {
                kind: EffectKind::Recall(r),
                name,
                ..
            }) => Some((r.clone(), name.map_or(0, |(_, table)| table))),
            _ => None,
        })
}

/// Is this skill a recall?
#[must_use]
pub fn is_recall<S: Store>(s: &Server<S>, skill: u32) -> bool {
    recall_of(s, skill).is_some()
}

/// The places a recall may take a player now: the place it keeps of the link's kind; the named
/// places the world has; none for an allegiance's hall (no allegiance is bound to one).
#[must_use]
pub fn recall_places<S: Store>(s: &Server<S>, world: ConnId, r: &Recall) -> Vec<StoredPosition> {
    let Some(p) = s.players.get(&world) else {
        return Vec::new();
    };
    match r {
        Recall::Link(kind) => p
            .places
            .iter()
            .filter(|(k, _)| k == kind)
            .map(|(_, at)| *at)
            .collect(),
        Recall::Named { names, .. } => names
            .iter()
            .filter_map(|n| s.places_by_name.get(&n.to_ascii_lowercase()))
            .map(|pl| StoredPosition {
                cell: pl.cell,
                origin: pl.origin,
                rotation: pl.rotation,
            })
            .collect(),
        Recall::Allegiance => Vec::new(),
    }
}

/// Is this use's skill used on its user alone?
fn self_only(u: &Usage) -> bool {
    u.valid_targets == 1 || u.ui_flags & ui_flag::SELF_TARGETED != 0
}

/// Is a skill helpful: it does no damage and may be used on its user (a heal, a buff)?
#[must_use]
pub fn helpful(u: &Usage) -> bool {
    u.ui_flags & ui_flag::NON_DAMAGING != 0 && u.valid_targets & 1 != 0
}

/// Whom a use is aimed at: a skill used on its user alone, at it; a helpful skill at the player
/// the request names, else at its user (a creature, nothing or itself named); any other at the
/// other player the request names, else at the thing it names.
#[must_use]
pub fn aim_of<S: Store>(s: &Server<S>, world: ConnId, r: Request, u: &Usage) -> Aim {
    let me = s.players.get(&world).map_or(0, |p| p.character);
    if self_only(u) || (r.target == me && helpful(u)) {
        return Aim::Me;
    }
    if helpful(u) {
        return crate::presence::player_for(s, world, r.target)
            .map_or(Aim::Me, |c| Aim::Player(c, r.target));
    }
    match combat::player_target(s, world, r.target) {
        Some(c) => Aim::Player(c, r.target),
        None => Aim::Thing(r.target),
    }
}

/// A player's rating in a skill with its effects (the spellcraft of the skill's effects, what its
/// hooks add by).
pub(crate) fn rating_of<S: Store>(s: &Server<S>, world: ConnId, skill: u32) -> f32 {
    let Some(p) = s.players.get(&world) else {
        return 1.0;
    };
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let held = crate::skills::held(&starting, &p.play.skills);
    let held = if quals::int(&p.qualities, quality::VIGOR) == Some(0) {
        s.skill_table.at_no_vigor(held)
    } else {
        held
    };
    #[allow(clippy::cast_precision_loss)]
    let r = effective_rating(s, p, &held, skill) as f32;
    r.max(1.0)
}

/// A player's rating in a skill it holds, with what its effects make of it.
pub(crate) fn effective_rating<S: Store>(
    s: &Server<S>,
    p: &crate::world::Player,
    held: &[crate::skills::Held],
    skill: u32,
) -> u32 {
    let hero = crate::focus::is_hero(&p.qualities);
    s.skill_table.rating_with(held, skill, hero, &|sk, rating| {
        crate::effects::skill_rating(&s.effect_table, &p.play.effects, sk, rating)
    })
}

/// [`rating_of`] for tests.
#[doc(hidden)]
#[must_use]
pub fn rating_of_for_test<S: Store>(s: &Server<S>, world: ConnId, skill: u32) -> f32 {
    rating_of(s, world, skill)
}

/// Does a player carry an effect of each of these saved states?
fn carries<S: Store>(s: &Server<S>, world: ConnId, states: &[u32]) -> bool {
    s.players.get(&world).is_some_and(|p| {
        states
            .iter()
            .all(|st| p.play.effects.iter().any(|e| e.state == *st))
    })
}

/// May a player swing this basic attack: it carries all the effects the skill requires and none
/// that bars it (2017: 35,700 swings seen of alchemy's two kinds changed kind 23 times, about 10 s
/// apart; 120,326 basic attacks of capturing players, none under an effect barring it), and has
/// trained the skills it names (15,389 of 15,389 that name any, by the session's saved list).
pub(crate) fn may_swing<S: Store>(s: &Server<S>, world: ConnId, u: &Usage) -> bool {
    let Some(p) = s.players.get(&world) else {
        return false;
    };
    if !carries(s, world, &u.required_effects)
        || p.play
            .effects
            .iter()
            .any(|e| u.barring_effects.contains(&e.state))
    {
        return false;
    }
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let held = crate::skills::held(&starting, &p.play.skills);
    u.use_prereqs.iter().all(|(skill, rating)| {
        held.iter().any(|h| h.skill == *skill) && effective_rating(s, p, &held, *skill) >= *rating
    })
}

/// A missile hook of a skill, if it throws one.
pub(crate) fn missile_hook(u: &Usage) -> Option<&Hook> {
    u.hooks
        .iter()
        .find(|h| h.flags & hook_flag::PROJECTILE != 0 && h.missile != 0)
}

/// The hooks that throw a missile: their indexes among the skill's, and the hooks.
pub(crate) fn throws_of(u: &Usage) -> Vec<(usize, &Hook)> {
    u.hooks
        .iter()
        .enumerate()
        .filter(|(_, h)| throws(h))
        .collect()
}

/// The first hook that hurts its target.
fn damaging_hook(u: &Usage) -> Option<(usize, &Hook)> {
    if u.ui_flags & ui_flag::NON_DAMAGING != 0 {
        return None;
    }
    u.hooks
        .iter()
        .enumerate()
        .find(|(_, h)| h.flags & hook_flag::NON_DAMAGING == 0)
}

/// How long a skill still waits, in seconds: its recovery since its last use, or the wait every
/// skill shares, whichever is longer.
#[must_use]
pub fn time_left<S: Store>(
    s: &Server<S>,
    world: ConnId,
    skill: u32,
    u: &Usage,
    now_game: f64,
) -> f64 {
    let Some(p) = s.players.get(&world) else {
        return 0.0;
    };
    let recovery = p
        .last_used
        .get(&skill)
        .map_or(0.0, |t| f64::from(u.recovery) - (now_game - t));
    let reset = if u.ui_flags & ui_flag::IGNORE_RESET == 0 {
        let q = quals::read(&p.qualities);
        match (
            q.floats.get(q::RESET_DURATION).copied(),
            q.timestamps.get(q::LAST_RESET).copied(),
        ) {
            (Some(d), Some(t)) => f64::from(d) - (now_game - t),
            _ => 0.0,
        }
    } else {
        0.0
    };
    recovery.max(reset)
}

/// Refuse a use: the attack state raised (unless it is), the use over, why, and lowered again.
pub fn refuse<S: Store>(s: &Server<S>, net: &mut ServerNet, world: ConnId, r: Request, err: u32) {
    let attacking = s
        .players
        .get(&world)
        .is_some_and(|p| p.attacking.is_some() || p.using.is_some());
    if !attacking {
        progress::private(net, world, &combat::attack_state(true));
    }
    progress::private(net, world, &ended(r.id));
    progress::private(net, world, &refused(err, r.target, r.skill));
    if !attacking {
        progress::private(net, world, &combat::attack_state(false));
    }
}

/// The client asks to use a skill.
pub fn request<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let Some(r) = Request::read(v) else {
        s.unhandled.note("a skill's use that does not read");
        return;
    };
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if p.in_run.is_some() {
        // A special waiting for the run's next swing is replaced by the newer one.
        if let Some(old) = p.in_run.replace(r) {
            progress::private(net, world, &ended(old.id));
        }
        return;
    }
    if p.using.is_some() {
        // One request waits while a use runs: a newer one replaces it.
        if let Some(old) = p.queued.replace(r) {
            progress::private(net, world, &ended(old.id));
        }
        return;
    }
    if p.attacking.is_some() {
        // Inside a run: the special waits for the run's next swing, where it is checked.
        p.in_run = Some(r);
        return;
    }
    starts(s, net, world, r, clock);
}

/// The checks of a use, first failure first: its usage, or why it is refused.
fn check<S: Store>(
    s: &mut Server<S>,
    _net: &mut ServerNet,
    world: ConnId,
    r: Request,
    clock: Clock,
) -> Result<Usage, u32> {
    let now_game = _net.game_time(clock.local);
    let Some(p) = s.players.get(&world) else {
        return Err(refusal::INTERRUPTED);
    };
    let Some(u) = s
        .skill_table
        .skills
        .get(&r.skill)
        .and_then(|d| d.usage.clone())
    else {
        return Err(refusal::INTERRUPTED);
    };
    if combat::is_dead(p) {
        return Err(refusal::DEAD);
    }
    if !implement_allowed(&s.catalog, &u, hands(s, p.character)) {
        return Err(refusal::WRONG_IMPLEMENT);
    }
    let vigor = quals::int(&p.qualities, quality::VIGOR).unwrap_or(0);
    let max_vigor = quals::int(&p.qualities, quality::VIGOR_MAX).unwrap_or(vigor);
    #[allow(clippy::cast_precision_loss)]
    if (vigor as f32) < u.min_vigor {
        return Err(refusal::NO_VIGOR);
    }
    if time_left(s, world, r.skill, &u, now_game) > 0.0 {
        return Err(refusal::NOT_RESET);
    }
    if vigor < cost_now(s, p, &u, max_vigor) {
        return Err(refusal::NOT_ENOUGH_VIGOR);
    }
    if !carries(s, world, &u.required_effects) {
        return Err(refusal::MISSING_EFFECT);
    }
    let walks = u.ui_flags & ui_flag::MOVE_TO != 0;
    match aim_of(s, world, r, &u) {
        Aim::Me => {}
        Aim::Player(c, other) => {
            // A hostile skill at another player: only as who may harm whom allows.
            if !helpful(&u) {
                combat::may_attack_player(s, world, c)?;
            }
            let at = s
                .players
                .values()
                .find(|q| q.character == other)
                .map(|q| q.position);
            let d = at.and_then(|a| {
                crate::reach::gap(&p.position, crate::reach::PLAYER, &a, crate::reach::PLAYER)
            });
            if !walks && d.is_none_or(|d| d > u.max_range) {
                return Err(refusal::TOO_FAR);
            }
            if !walks && !helpful(&u) && u.min_range > 0.0 && d.is_some_and(|d| d < u.min_range) {
                return Err(refusal::TOO_CLOSE);
            }
        }
        Aim::Thing(target) => {
            if target == 0 {
                return Err(refusal::TARGET_GONE);
            }
            let Some(t) = s.things.get(target) else {
                return Err(refusal::TARGET_GONE);
            };
            if !t.alive() {
                return Err(refusal::TARGET_DEAD);
            }
            let Some(at) = t.at else {
                return Err(refusal::TARGET_GONE);
            };
            if !walks {
                let body = crate::reach::body_of(s, target);
                let d = crate::reach::gap(&p.position, crate::reach::PLAYER, &at, body)
                    .unwrap_or(f32::MAX);
                if d > u.max_range {
                    return Err(refusal::TOO_FAR);
                }
                if u.min_range > 0.0 && d < u.min_range {
                    return Err(refusal::TOO_CLOSE);
                }
            }
        }
    }
    if u.ui_flags & ui_flag::USE_WHILE_MOVING == 0 && moving(p) {
        return Err(refusal::MOVING);
    }
    if crate::pets::too_many(s, p.character, &u.user_effects) {
        return Err(crate::pets::TOO_MANY_PETS);
    }
    Ok(u)
}

/// A use begins: refused, or under way.
pub fn starts<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    r: Request,
    clock: Clock,
) {
    if toggles_off(s, net, world, r) {
        return;
    }
    let u = match check(s, net, world, r, clock) {
        Ok(u) => u,
        Err(err) => {
            refuse(s, net, world, r, err);
            return;
        }
    };
    let began = net.game_time(clock.local);
    let ts = use_time_scale(s, world, &u);
    let aim = aim_of(s, world, r, &u);
    let spellcraft = rating_of(s, world, r.skill);
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let me = p.character;
    let mode = player_mode(s, me);
    progress::private(net, world, &combat::attack_state(true));
    // Attacking ends a lifestone protection, before the use's mode.
    crate::effects::unprotect(s, net, world, |_, _| {});
    combat::set_player_mode(s, net, world, mode);
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let mut turned = false;
    if u.powerup > 0.0 {
        let hold = u.powerup > 1.0;
        let mut b = BehaviorParams {
            bits: BehaviorParams::BEHAVIOR
                | 0x20
                | BehaviorParams::CONTEXT
                | if hold { BehaviorParams::HOLD_CYCLES } else { 0 },
            behavior: u.powerup_behavior,
            hold_cycles: if hold {
                dereth_primitives::num::to_i32(u.powerup)
                    .max(0)
                    .cast_unsigned()
            } else {
                0
            },
            context,
            ..BehaviorParams::default()
        };
        b.words[1] = 1;
        let plays = ServerMessage::PlayBehavior(player_event(p, b));
        to_all_seeing(s, net, me, &plays);
    } else if let Some(target) = aimed_at(aim) {
        let order = if missile_hook(&u).is_some() || u.max_range > 1.0 {
            turn_to(p, target, context)
        } else {
            approach(p, target, u.max_range, context)
        };
        turned = true;
        to_all_seeing(s, net, me, &order);
    }
    let story_in = if u.powerup > 0.0 {
        f64::from(u.powerup) * POWERUP_SCALE
    } else {
        STORY_AFTER
    };
    if let Some(p) = s.players.get_mut(&world) {
        p.using = Some(Using {
            req: r,
            usage: u,
            began,
            alone: true,
            told: false,
            time_scale: ts,
            outcome: None,
            aim,
            turned,
            spellcraft,
            landings: Vec::new(),
        });
    }
    s.after(
        clock,
        story_in,
        Timer::Skill {
            world,
            id: r.id,
            step: Step::Story,
        },
    );
}

/// The aura a skill turns on (its user effect that is one), if it is a toggled skill.
fn aura_of<S: Store>(s: &Server<S>, skill: u32) -> Option<u32> {
    let u = s.skill_table.skills.get(&skill)?.usage.as_ref()?;
    u.user_effects
        .iter()
        .copied()
        .find(|st| crate::effects::is_aura(s, *st))
}

/// A toggled skill used while its aura is on turns it off, in one tick: attacking (unless it
/// was), the use over, the toggle off, the aura's records taken off, "You stop …" with the skill's
/// name, not attacking again (unless it was), the appearance stopped. Returns whether it did.
fn toggles_off<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    r: Request,
) -> bool {
    let Some(aura) = aura_of(s, r.skill) else {
        return false;
    };
    let Some(rec) = crate::effects::aura_held(s, world, aura) else {
        return false;
    };
    let attacking = s
        .players
        .get(&world)
        .is_some_and(|p| p.attacking.is_some() || p.using.is_some());
    if !attacking {
        progress::private(net, world, &combat::attack_state(true));
    }
    progress::private(net, world, &ended(r.id));
    progress::private(net, world, &crate::effects::toggle_call(false, r.skill));
    let stop = crate::effects::aura_off(s, net, world, rec.eid);
    if let Some(name) = s.skill_table.skills.get(&r.skill).map(|d| d.name) {
        let text = progress::text_with(
            crate::effects::toggled_off::TEXT,
            vec![progress::nested(
                crate::effects::toggled_off::SKILL_VAR,
                progress::text(name),
            )],
        );
        progress::private(net, world, &crate::chat::show_text(2, text));
    }
    if !attacking {
        progress::private(net, world, &combat::attack_state(false));
    }
    crate::effects::stop_fx(s, net, world, &stop);
    true
}

/// A movement order turning the player to its target.
pub(crate) fn turn_to(p: &mut crate::world::Player, target: u64, context: u32) -> ServerMessage {
    p.move_to_stamp = p.move_to_stamp.wrapping_add(1);
    let stamp = p.move_to_stamp;
    ServerMessage::MoveTo(player_event(
        p,
        MoveOrder {
            params: MovementParams {
                flags: 0x46A,
                desired_velocity: 0.0,
                fail_distance: 0.0,
                desired_distance: 0.0,
                max_success_distance: 0.0,
                min_success_distance: 0.0,
                target: MoveTarget::Object(target),
                offset: Vec3::ZERO,
                offset_id: 0,
                waypoints: Vec::new(),
                context,
            },
            stamp,
        },
    ))
}

/// A movement order walking the player to within a skill's reach of its target.
fn approach(p: &mut crate::world::Player, target: u64, reach: f32, context: u32) -> ServerMessage {
    p.move_to_stamp = p.move_to_stamp.wrapping_add(1);
    let stamp = p.move_to_stamp;
    ServerMessage::MoveTo(player_event(
        p,
        MoveOrder {
            params: MovementParams {
                flags: 0x46B,
                desired_velocity: 1.0,
                fail_distance: f32::MAX,
                desired_distance: reach,
                max_success_distance: reach,
                min_success_distance: 0.0,
                target: MoveTarget::Object(target),
                offset: Vec3::ZERO,
                offset_id: 0,
                waypoints: Vec::new(),
                context,
            },
            stamp,
        },
    ))
}

/// A movement order stopping the player.
pub(crate) fn stop_moving(p: &mut crate::world::Player) -> ServerMessage {
    p.move_to_stamp = p.move_to_stamp.wrapping_add(1);
    let stamp = p.move_to_stamp;
    ServerMessage::MoveTo(player_event(
        p,
        MoveOrder {
            params: MovementParams {
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
            stamp,
        },
    ))
}

/// A run's swing slot taken by the special waiting in it (the run has sent its movement order):
/// checked again, then its story; the run's next swing after the special's end. Refused, the run
/// swings on.
pub fn slot<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    r: Request,
    clock: Clock,
) -> bool {
    let u = match check(s, net, world, r, clock) {
        Ok(u) => u,
        Err(err) => {
            refuse(s, net, world, r, err);
            return false;
        }
    };
    let began = net.game_time(clock.local);
    let ts = use_time_scale(s, world, &u);
    let aim = aim_of(s, world, r, &u);
    let spellcraft = rating_of(s, world, r.skill);
    let Some(p) = s.players.get_mut(&world) else {
        return false;
    };
    if let Some(run) = p.attacking.as_mut() {
        run.next_swing = f64::INFINITY;
    }
    let powerup = (u.powerup > 0.0).then_some((u.powerup, u.powerup_behavior));
    p.using = Some(Using {
        req: r,
        usage: u,
        began,
        alone: false,
        told: false,
        time_scale: ts,
        outcome: None,
        aim,
        turned: false,
        spellcraft,
        landings: Vec::new(),
    });
    match powerup {
        // A skill that powers up (a recall) does so first, as from rest.
        Some((time, behavior)) => {
            s.next_context = s.next_context.wrapping_add(1);
            let context = s.next_context;
            if let Some(p) = s.players.get_mut(&world) {
                // Held for its cycles, as from rest.
                let hold = time > 1.0;
                let mut b = BehaviorParams {
                    bits: BehaviorParams::BEHAVIOR
                        | 0x20
                        | BehaviorParams::CONTEXT
                        | if hold { BehaviorParams::HOLD_CYCLES } else { 0 },
                    behavior,
                    hold_cycles: if hold {
                        dereth_primitives::num::to_i32(time).max(0).cast_unsigned()
                    } else {
                        0
                    },
                    context,
                    ..BehaviorParams::default()
                };
                b.words[1] = 1;
                let me = p.character;
                let plays = ServerMessage::PlayBehavior(player_event(p, b));
                to_all_seeing(s, net, me, &plays);
            }
            s.after(
                clock,
                f64::from(time) * POWERUP_SCALE,
                Timer::Skill {
                    world,
                    id: r.id,
                    step: Step::Story,
                },
            );
        }
        None => story(s, net, world, r.id, clock),
    }
    true
}

/// A step of a use, when it is due.
pub fn step<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    id: u32,
    st: Step,
    clock: Clock,
) {
    if st == Step::Arrives {
        return arrives(s, net, world, id, clock);
    }
    let current = s
        .players
        .get(&world)
        .and_then(|p| p.using.as_ref())
        .is_some_and(|u| u.req.id == id);
    if !current {
        return;
    }
    match st {
        Step::Story => story(s, net, world, id, clock),
        Step::Strike => strike(s, net, world, clock),
        Step::Arrives => {}
        Step::Portal => recall_portal(s, net, world, clock),
        Step::Out => recall_out(s, net, world, clock),
        Step::Summon => summon_portal(s, net, world, clock),
        Step::Hook => hook(s, net, world, clock),
        Step::End => end(s, net, world, clock),
    }
}

/// Pay a use's vigor: the vigor after it sent, and "You are fatigued." when none is left.
fn pay<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, u: &Usage) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let vigor = quals::int(&p.qualities, quality::VIGOR).unwrap_or(0);
    let max = quals::int(&p.qualities, quality::VIGOR_MAX).unwrap_or(vigor);
    let left = (vigor - cost_now(s, p, u, max)).max(0);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    quals::set_int(&mut p.qualities, quality::VIGOR, left);
    progress::send_int(net, world, quality::VIGOR, left);
    if left == 0 {
        progress::say(net, world, progress::text(text::FATIGUED));
    }
}

/// The hooks of a story of a skill's use: kind 2 first, then one per hook of the skill: one that
/// only plays (with its target and a hit), a missile (naming the weapon that throws it), or a
/// strike with its result (and the health change when it hurts); a hook after the first carries
/// its index.
pub(crate) fn story_hooks(
    u: &Usage,
    target: u64,
    weapon: u64,
    outcome: Option<(Outcome, crate::effects::Harm)>,
) -> Vec<StoryHook> {
    let (o, harm) = outcome.unwrap_or((Outcome::Hit, crate::effects::Harm::default()));
    let landings: Vec<Landing> = (0..u.hooks.len())
        .map(|hook| Landing {
            target,
            hook,
            outcome: o,
            damage: harm.left,
            back: harm.back,
        })
        .collect();
    story_hooks_with(u, target, weapon, &landings)
}

/// The hooks of a basic attack that strike by their own hand: their indexes among the skill's.
#[must_use]
pub fn strikes(u: &Usage) -> Vec<usize> {
    u.hooks
        .iter()
        .enumerate()
        .filter(|(_, h)| !plays_only(h) && !throws(h))
        .map(|(i, _)| i)
        .collect()
}

/// Does a hook only play (no target to strike)?
fn plays_only(h: &Hook) -> bool {
    h.flags & hook_flag::NON_DAMAGING != 0 && h.flags & hook_flag::PROJECTILE == 0
}

/// Is a hook a missile's?
fn throws(h: &Hook) -> bool {
    h.flags & hook_flag::PROJECTILE != 0 && h.missile != 0
}

/// [`story_hooks`] with each striking hook's landings: one strike per target it reached, in turn
/// (a hook with none strikes its target with a hit).
pub(crate) fn story_hooks_with(
    u: &Usage,
    target: u64,
    weapon: u64,
    landings: &[Landing],
) -> Vec<StoryHook> {
    let mut hooks = vec![StoryHook {
        bits: story_hook::OPENING,
        ..StoryHook::default()
    }];
    for (i, h) in u.hooks.iter().enumerate() {
        let index = u32::try_from(i).unwrap_or(0);
        let extra = if i > 0 { story_hook::ATTACK_HOOK } else { 0 };
        if plays_only(h) {
            let mut sh = StoryHook {
                bits: story_hook::PLAYS,
                target,
                ..StoryHook::default()
            };
            sh.numbers[0] = Outcome::Hit.code().cast_signed();
            hooks.push(sh);
            continue;
        }
        if throws(h) {
            let mut sh = StoryHook {
                bits: story_hook::MISSILE | extra,
                target,
                weapon,
                ..StoryHook::default()
            };
            sh.numbers[1] = h.anim_hook.cast_signed();
            sh.numbers[2] = index.cast_signed();
            hooks.push(sh);
            continue;
        }
        let mine: Vec<Landing> = landings.iter().filter(|l| l.hook == i).copied().collect();
        let mine = if mine.is_empty() {
            vec![Landing {
                target,
                hook: i,
                outcome: Outcome::Hit,
                damage: 0,
                back: 0,
            }]
        } else {
            mine
        };
        for l in mine {
            let hurts = l.outcome.landed() && l.damage > 0;
            let mut sh = StoryHook {
                bits: if hurts {
                    story_hook::STRIKE_HIT
                } else {
                    story_hook::STRIKE_MISSED
                } | extra,
                target: l.target,
                ..StoryHook::default()
            };
            sh.numbers[0] = l.outcome.code().cast_signed();
            sh.numbers[1] = h.anim_hook.cast_signed();
            sh.numbers[2] = index.cast_signed();
            if hurts {
                sh.numbers[3] = -l.damage;
            }
            if l.back > 0 {
                sh.bits |= crate::combat::ATTACKER_HEALTH;
                sh.numbers[4] = -l.back;
            }
            hooks.push(sh);
        }
    }
    hooks
}

/// The live creatures a hook with an area reaches: those whose bodies are within its radius of
/// its center's body, by id.
fn area_targets<S: Store>(
    s: &Server<S>,
    center: &yalain_store::StoredPosition,
    body: crate::reach::Body,
    radius: f32,
) -> Vec<u64> {
    let mut out: Vec<u64> = s
        .things
        .placed()
        .filter(|t| t.alive() && t.creature.is_some())
        .filter(|t| {
            t.at.as_ref().is_some_and(|at| {
                crate::reach::gap(center, body, at, crate::reach::body_of(s, t.id))
                    .is_some_and(|d| d <= radius)
            })
        })
        .map(|t| t.id)
        .collect();
    out.sort_unstable();
    out
}

/// A hitter as a blow of this hook is reckoned: with what the hook adds at its rating, a critical
/// taking it with the rest of the hit (2017, a basic attack of 351 criticals: 2.09 times its plain
/// hit; a special's: [`rules::SPECIAL_CRITICAL`]).
pub(crate) fn hitting_with(me: rules::Hitter, h: &Hook, rating: f32) -> rules::Hitter {
    rules::Hitter {
        added: me.added + h.rated_add(rating),
        ..me
    }
}

/// A hitter as a blow of this hook is reckoned by the hand that strikes: one of the left hand does
/// the damage of what the left hand holds, a shield's too (2017, Victimize with a shield recorded:
/// median 99, 103 by the shield and 171 by the right hand's weapon, 32 hits). The right hand's, when
/// the left holds nothing with damage.
fn by_its_hand<S: Store>(
    s: &Server<S>,
    world: ConnId,
    me: rules::Hitter,
    h: &Hook,
) -> rules::Hitter {
    if h.flags & hook_flag::LEFT_HAND == 0 {
        return me;
    }
    let Some(p) = s.players.get(&world) else {
        return me;
    };
    let Some((left, _)) = hands(s, p.character).left else {
        return me;
    };
    combat::weapon_damage(s, left).map_or(me, |weapon| rules::Hitter {
        weapon: Some(weapon),
        critical: combat::critical_bonus(s, Some(left)),
        ..me
    })
}

/// What a special's hook does on a creature when it lands.
fn hook_hit<S: Store>(
    s: &mut Server<S>,
    me: rules::Hitter,
    h: &Hook,
    (target, armor): (u64, i32),
    o: Outcome,
) -> i32 {
    let base = rules::player_hit(&mut s.rng, me, armor, false) + combat::armor_taken(s, target);
    #[allow(clippy::cast_precision_loss)]
    let d = (base as f32 + h.add) * if h.mult > 0.0 { h.mult } else { 1.0 };
    let d = if o == Outcome::Critical {
        d * rules::SPECIAL_CRITICAL
    } else {
        d
    };
    dereth_primitives::num::to_i32(d.round()).max(1)
}

/// How one hook comes out on one creature: a hit, critical, miss or evade by the rules (a strike
/// is evaded, never missed), and the damage when it lands and hurts.
fn roll_hook<S: Store>(
    s: &mut Server<S>,
    u: &Usage,
    h: &Hook,
    me: rules::Hitter,
    target: u64,
) -> Option<(Outcome, i32)> {
    let (level, armor) = match crate::social::world_of(s, target) {
        Some(c) => s
            .players
            .get(&c)
            .map(|q| (crate::examine::level_of(&q.qualities), 0))?,
        None => s
            .things
            .get(target)
            .map(|t| (t.level(), t.creature.as_ref().map_or(0, |c| c.stats.armor)))?,
    };
    let kind = if h.flags & hook_flag::PROJECTILE != 0 {
        Kind::Missile
    } else {
        Kind::Melee
    };
    let o = rules::roll_special(&mut s.rng, kind, me.level, level, me.critical);
    let o = if kind == Kind::Melee && o == Outcome::Miss {
        Outcome::Evade
    } else {
        o
    };
    let hurts = u.ui_flags & ui_flag::NON_DAMAGING == 0 && h.flags & hook_flag::NON_DAMAGING == 0;
    let dmg = if o.landed() && hurts {
        hook_hit(s, me, h, (target, armor), o)
    } else {
        0
    };
    Some((o, dmg))
}

/// The time scale a player tells a use at: its own, times the square of what the skill's
/// definition says of its speed (2017: Sinew Snap's 1.6 told 2.56 x its session's other stories, 2.50
/// to 2.62 over 9 sessions and 170 stories).
pub(crate) fn use_time_scale<S: Store>(s: &mut Server<S>, world: ConnId, u: &Usage) -> f32 {
    combat::player_time_scale(s, world) * u.combat_speed.map_or(1.0, |m| m * m)
}

/// The behavior params of a special's story.
pub(crate) fn story_params(behavior: u32, ts: f32) -> BehaviorParams {
    let mut b = BehaviorParams {
        bits: STORY_BEHAVIOR_BITS,
        behavior,
        time_scale: ts,
        ..BehaviorParams::default()
    };
    if WITH_WORD_20.contains(&behavior) {
        b.bits |= 0x20;
        b.words[1] = 1;
    }
    b.words[7] = 1;
    b.words[9] = 0;
    b
}

/// The story of a use: its vigor paid, the story told to all who see the player, the wielded
/// things playing the behavior; then the strike's landing, the missile's arrival, the recall's
/// portal, or the end.
fn story<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, id: u32, clock: Clock) {
    let Some((r, u, ts, aim)) = s
        .players
        .get(&world)
        .and_then(|p| p.using.as_ref())
        .map(|x| (x.req, x.usage.clone(), x.time_scale, x.aim))
    else {
        return;
    };
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let me = p.character;
    let my_level = crate::examine::level_of(&p.qualities);
    let me_hits = combat::hitter(s, p);
    let rating = rating_of(s, world, r.skill);
    let held = wielded(s, me);
    let on_self = aim == Aim::Me;
    let target = aimed_at(aim).unwrap_or(me);
    // A strike that walks the player to its target does not reach from afar: wait for the player
    // to come into reach.
    if !on_self && missile_hook(&u).is_none() && u.ui_flags & ui_flag::MOVE_TO != 0 {
        let at = position_of(s, aim, target);
        let d = at.and_then(|a| crate::presence::distance(&p.position, &a));
        match d {
            None => return interrupt(s, net, world, refusal::TARGET_GONE, clock),
            Some(d) if d > combat::PLAYER_REACH => {
                s.after(
                    clock,
                    0.2,
                    Timer::Skill {
                        world,
                        id,
                        step: Step::Story,
                    },
                );
                return;
            }
            Some(_) => {}
        }
    }
    if u.powerup > 0.0 {
        if let Some(p) = s.players.get_mut(&world) {
            let stop = ServerMessage::StopBehavior(player_event(p, Word(u.powerup_behavior)));
            to_all_seeing(s, net, me, &stop);
        }
    }
    pay(s, net, world, &u);
    // A special used on another fights: what binds when used in combat is bound.
    if !on_self && !helpful(&u) {
        crate::equip::in_combat(s, world);
    }
    // The outcome of its damaging hook (a strike's at once; a missile's as it arrives).
    let target_thing = match aim {
        // Another player: its level, no armor.
        Aim::Player(c, _) if !helpful(&u) => s
            .players
            .get(&c)
            .map(|q| (crate::examine::level_of(&q.qualities), 0)),
        _ => s
            .things
            .get(target)
            .map(|t| (t.level(), t.creature.as_ref().map_or(0, |c| c.stats.armor))),
    };
    let mut outcome = None;
    if let (Some((_, h)), Some((level, armor))) = (damaging_hook(&u), target_thing) {
        let kind = if h.flags & hook_flag::PROJECTILE != 0 {
            Kind::Missile
        } else {
            Kind::Melee
        };
        let me_hits = by_its_hand(s, world, me_hits, h);
        let o = rules::roll_special(&mut s.rng, kind, my_level, level, me_hits.critical);
        // Strikes of specials are evaded, never missed.
        let o = if kind == Kind::Melee && o == Outcome::Miss {
            Outcome::Evade
        } else {
            o
        };
        let dmg = if o.landed() {
            hook_hit(s, hitting_with(me_hits, h, rating), h, (target, armor), o)
        } else {
            0
        };
        outcome = Some((o, dmg));
    }
    let weapon = held.first().map_or(0, |(i, _)| *i);
    let landings = if helpful(&u) {
        Vec::new()
    } else {
        landings_of(s, world, &u, aim, target, outcome, (me_hits, rating))
    };
    let hooks = story_hooks_with(&u, target, weapon, &landings);
    let hostile_at_player = matches!(aim, Aim::Player(..)) && !helpful(&u);
    let behavior = if on_self {
        u.behavior_self[0]
    } else {
        u.behavior_other[0]
    };
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let told = ServerMessage::DoStory(player_event(
        p,
        PhysicsStory {
            bits: SPECIAL_STORY,
            attacker: me,
            attacker_behavior: story_params(behavior, ts),
            attack_context: r.id,
            skill: r.skill,
            hooks,
        },
    ));
    if let Some(x) = p.using.as_mut() {
        x.told = true;
        x.outcome = outcome;
        x.landings.clone_from(&landings);
    }
    let mut told = told;
    if hostile_at_player {
        combat::with_pk_damage(&mut told, target);
    }
    to_all_seeing(s, net, me, &told);
    for (item, _) in &held {
        let plays = ServerMessage::PlayBehavior(ObjectEvent {
            sender: crate::inventory::visual(s, *item),
            action: BehaviorParams::behavior(behavior),
        });
        to_all_seeing(s, net, me, &plays);
    }
    let after = |x: f64| x / f64::from(ts);
    if crate::usable::portals::summons(r.skill).is_some() {
        s.after(
            clock,
            crate::usable::portals::APPEARS_AFTER,
            Timer::Skill {
                world,
                id,
                step: Step::Summon,
            },
        );
        return;
    }
    if let Some((recall, _)) = recall_of(s, r.skill) {
        let bound = !recall_places(s, world, &recall).is_empty();
        let wait = if bound {
            RECALL_TO_PORTAL
        } else {
            RECALL_FAILS_AFTER
        };
        s.after(
            clock,
            wait,
            Timer::Skill {
                world,
                id,
                step: Step::Portal,
            },
        );
        return;
    }
    if helpful(&u) && !u.target_effects.is_empty() {
        s.after(
            clock,
            after(hook_seconds(behavior, on_self)),
            Timer::Skill {
                world,
                id,
                step: Step::Hook,
            },
        );
    }
    if let Some(h) = missile_hook(&u) {
        if let Some(p) = s.players.get_mut(&world) {
            let spellcraft = p.using.as_ref().map_or(1.0, |x| x.spellcraft);
            p.flying.push(Flying {
                req: r,
                usage: u.clone(),
                outcome,
                spellcraft,
            });
        }
        let d = position_of(s, aim, target)
            .zip(s.players.get(&world).map(|p| p.position))
            .and_then(|(a, b)| crate::presence::distance(&b, &a))
            .unwrap_or(0.0);
        let speed = if h.missile_speed > 0.0 {
            h.missile_speed
        } else {
            35.0
        };
        s.after(
            clock,
            MISSILE_WINDUP + f64::from(d / speed),
            Timer::Skill {
                world,
                id,
                step: Step::Arrives,
            },
        );
    } else if !landings.is_empty() {
        s.after(
            clock,
            after(STRIKE_LANDS),
            Timer::Skill {
                world,
                id,
                step: Step::Strike,
            },
        );
    }
    s.after(
        clock,
        after(animation_seconds(behavior)),
        Timer::Skill {
            world,
            id,
            step: Step::End,
        },
    );
}

/// How a hostile use's striking hooks come out: a hook with an area on every creature it reaches
/// around its center (the target, or the user), and on the other player it is aimed at; any other
/// on the target when it is a creature or another player (its damaging hook's outcome already
/// rolled).
fn landings_of<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    u: &Usage,
    aim: Aim,
    target: u64,
    rolled: Option<(Outcome, i32)>,
    (me, rating): (rules::Hitter, f32),
) -> Vec<Landing> {
    let center = match aim {
        Aim::Thing(t) => s
            .things
            .get(t)
            .and_then(|x| x.at)
            .map(|at| (at, crate::reach::body_of(s, t))),
        Aim::Player(c, _) => s
            .players
            .get(&c)
            .map(|p| (p.position, crate::reach::PLAYER)),
        Aim::Me => s
            .players
            .get(&world)
            .map(|p| (p.position, crate::reach::PLAYER)),
    };
    let primary_creature = match aim {
        Aim::Player(c, _) => s
            .players
            .get(&c)
            .is_some_and(|q| q.dying.is_none() && !combat::is_dead(q)),
        _ => s
            .things
            .get(target)
            .is_some_and(|t| t.alive() && t.creature.is_some()),
    };
    let first_damaging = damaging_hook(u).map(|(i, _)| i);
    let mut out = Vec::new();
    for (i, h) in u.hooks.iter().enumerate() {
        if plays_only(h) || throws(h) {
            continue;
        }
        let targets = if h.flags & hook_flag::AREA != 0 && h.radius > 0.0 {
            let mut around =
                center.map_or_else(Vec::new, |(at, body)| area_targets(s, &at, body, h.radius));
            // Aimed at another player, it reaches that player first.
            if primary_creature && matches!(aim, Aim::Player(..)) {
                around.insert(0, target);
            }
            around
        } else if primary_creature {
            vec![target]
        } else {
            Vec::new()
        };
        for t in targets {
            let o = match rolled {
                Some(r) if t == target && first_damaging.is_some() => Some(r),
                _ => {
                    let me = by_its_hand(s, world, me, h);
                    roll_hook(s, u, h, hitting_with(me, h, rating), t)
                }
            };
            if let Some((outcome, damage)) = o {
                let harm = if outcome.landed() {
                    let other = combat::player_target(s, world, t);
                    let on =
                        other.map_or(crate::effects::On::Creature(t), crate::effects::On::Player);
                    crate::effects::answer_harm(s, on, damage, other.is_some())
                } else {
                    crate::effects::Harm::plain(damage)
                };
                out.push(Landing {
                    target: t,
                    hook: i,
                    outcome,
                    damage: harm.left,
                    back: harm.back,
                });
            }
        }
    }
    out
}

/// Where the one a use is aimed at stands: another player, or a thing.
fn position_of<S: Store>(s: &Server<S>, aim: Aim, target: u64) -> Option<StoredPosition> {
    match aim {
        Aim::Player(c, _) => s.players.get(&c).map(|p| p.position),
        _ => s.things.get(target).and_then(|t| t.at),
    }
}

/// The one a use is aimed at, other than its user.
#[must_use]
pub const fn aimed_at(aim: Aim) -> Option<u64> {
    match aim {
        Aim::Me => None,
        Aim::Player(_, id) | Aim::Thing(id) => Some(id),
    }
}

/// A helpful skill's effects reach its target: each of its target effects put on it, cast by the
/// user at its rating.
fn hook<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some((u, aim, spellcraft)) = s
        .players
        .get(&world)
        .and_then(|p| p.using.as_ref())
        .map(|x| (x.usage.clone(), x.aim, x.spellcraft))
    else {
        return;
    };
    let me = s.players.get(&world).map_or(0, |p| p.character);
    let on = match aim {
        Aim::Me => world,
        Aim::Player(c, _) => c,
        Aim::Thing(_) => return,
    };
    for state in &u.target_effects {
        crate::effects::apply(
            s,
            net,
            on,
            *state,
            crate::effects::Cast {
                caster: me,
                spellcraft,
                pk: true,
                ..crate::effects::Cast::default()
            },
            clock,
        );
    }
}

/// A use's strikes land: each target it landed on hurt, then the skill's target effects put on
/// each (once each); each still alive goes after the player.
fn strike<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some((u, landings, spellcraft)) = s
        .players
        .get(&world)
        .and_then(|p| p.using.as_ref())
        .map(|x| (x.usage.clone(), x.landings.clone(), x.spellcraft))
    else {
        return;
    };
    let me = s.players.get(&world).map_or(0, |p| p.character);
    for l in &landings {
        if !(l.outcome.landed() && (l.damage > 0 || l.back > 0)) {
            continue;
        }
        if let Some(c) = combat::player_target(s, world, l.target) {
            if s.players.get(&c).is_some_and(|q| q.dying.is_none()) {
                if l.damage > 0 {
                    combat::hurt_player(s, net, c, me, l.damage, clock);
                }
                crate::effects::harm_back(s, net, me, l.target, l.back, clock);
            }
        } else if s.things.get(l.target).is_some_and(|t| t.alive()) {
            if l.damage > 0 {
                combat::hurt_creature(s, net, l.target, me, l.damage, clock);
            }
            crate::effects::harm_back(s, net, me, l.target, l.back, clock);
        }
    }
    let mut hit: Vec<u64> = Vec::new();
    for l in &landings {
        if l.outcome.landed() && !hit.contains(&l.target) {
            hit.push(l.target);
        }
    }
    for t in &hit {
        let on = combat::player_target(s, world, *t)
            .map_or(crate::effects::On::Creature(*t), crate::effects::On::Player);
        put_target_effects(s, net, me, &u, on, spellcraft, clock);
        if matches!(on, crate::effects::On::Creature(_)) {
            crate::treasure::on_hit(s, net, world, *t, clock);
        }
    }
    let mut reached: Vec<u64> = Vec::new();
    for l in &landings {
        if !reached.contains(&l.target) {
            reached.push(l.target);
        }
    }
    for t in reached {
        if s.things.get(t).is_some_and(|x| x.alive()) {
            crate::ai::go_after(s, net, t, me, clock);
        }
    }
}

/// A skill's target effects put on a creature or another player it landed on, cast by the player
/// at its rating.
fn put_target_effects<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    me: u64,
    u: &Usage,
    on: crate::effects::On,
    spellcraft: f32,
    clock: Clock,
) {
    for state in &u.target_effects {
        crate::effects::apply_on(
            s,
            net,
            on,
            *state,
            crate::effects::Cast {
                caster: me,
                spellcraft,
                pk: true,
                ..crate::effects::Cast::default()
            },
            clock,
        );
    }
}

/// A missile arrives: the target tells how it came out (its health first when it is hurt).
fn arrives<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, id: u32, clock: Clock) {
    let Some(f) = s.players.get_mut(&world).and_then(|p| {
        let i = p.flying.iter().position(|f| f.req.id == id)?;
        Some(p.flying.remove(i))
    }) else {
        return;
    };
    let (r, u, outcome, spellcraft) = (f.req, f.usage, f.outcome, f.spellcraft);
    let me = s.players.get(&world).map_or(0, |p| p.character);
    let player = combat::player_target(s, world, r.target);
    let alive = |s: &Server<S>| match player {
        Some(c) => s.players.get(&c).is_some_and(|q| q.dying.is_none()),
        None => s.things.get(r.target).is_some_and(|t| t.alive()),
    };
    if !alive(s) {
        return;
    }
    let (o, dmg) = outcome.unwrap_or((Outcome::Evade, 0));
    let harm = if o.landed() {
        let on = player.map_or(
            crate::effects::On::Creature(r.target),
            crate::effects::On::Player,
        );
        crate::effects::answer_harm(s, on, dmg, player.is_some())
    } else {
        crate::effects::Harm::plain(dmg)
    };
    let dmg = harm.left;
    let hurts = o.landed() && dmg > 0;
    let mut hook = StoryHook {
        bits: if hurts {
            story_hook::STRIKE_HIT
        } else {
            story_hook::STRIKE_MISSED
        },
        target: r.target,
        ..StoryHook::default()
    };
    hook.numbers[0] = o.code().cast_signed();
    hook.numbers[1] = 1;
    if hurts {
        hook.numbers[3] = -dmg;
    }
    if harm.back > 0 {
        hook.bits |= crate::combat::ATTACKER_HEALTH;
        hook.numbers[4] = -harm.back;
    }
    let sender = match player {
        Some(c) => match s.players.get_mut(&c) {
            Some(q) => combat::player_event(q, ()).sender,
            None => return,
        },
        None => crate::inventory::visual(s, r.target),
    };
    let mut told = ServerMessage::DoStory(ObjectEvent {
        sender,
        action: PhysicsStory {
            bits: ARRIVAL_STORY,
            attacker: me,
            skill: r.skill,
            hooks: vec![hook],
            ..PhysicsStory::default()
        },
    });
    if player.is_some() {
        combat::with_pk_damage(&mut told, r.target);
    }
    to_all_seeing(s, net, r.target, &told);
    if let Some(c) = player {
        if hurts {
            combat::hurt_player(s, net, c, me, dmg, clock);
        }
        crate::effects::harm_back(s, net, me, r.target, harm.back, clock);
        if o.landed() && alive(s) {
            put_target_effects(
                s,
                net,
                me,
                &u,
                crate::effects::On::Player(c),
                spellcraft,
                clock,
            );
        }
        return;
    }
    if hurts {
        combat::hurt_creature(s, net, r.target, me, dmg, clock);
    }
    crate::effects::harm_back(s, net, me, r.target, harm.back, clock);
    if o.landed() && s.things.get(r.target).is_some_and(|t| t.alive()) {
        put_target_effects(
            s,
            net,
            me,
            &u,
            crate::effects::On::Creature(r.target),
            spellcraft,
            clock,
        );
        crate::treasure::on_hit(s, net, world, r.target, clock);
    }
    if s.things.get(r.target).is_some_and(|t| t.alive()) {
        crate::ai::go_after(s, net, r.target, me, clock);
    }
}

/// A recall's story is over: into portal space toward its place (one of several at random), or,
/// with none, told it failed (a city's recall, whose place the world lacks, says nothing).
fn recall_portal<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some((id, skill)) = s
        .players
        .get(&world)
        .and_then(|p| p.using.as_ref())
        .map(|u| (u.req.id, u.req.skill))
    else {
        return;
    };
    let Some((recall, table)) = recall_of(s, skill) else {
        return;
    };
    let places = recall_places(s, world, &recall);
    let to = match places.len() {
        0 => None,
        1 => places.first().copied(),
        n => places.get(s.rng.below(n)).copied(),
    };
    let named = matches!(recall, Recall::Named { .. });
    let Some(to) = to else {
        // A recall that fails puts nothing on its user (a city's protection).
        if let Some(u) = s.players.get_mut(&world).and_then(|p| p.using.as_mut()) {
            u.usage.user_effects.clear();
        }
        if !named {
            progress::private(
                net,
                world,
                &crate::chat::show_text(2, progress::text((text::RECALL_FAILED, table))),
            );
        }
        s.after(
            clock,
            FAILED_ENDS_AFTER,
            Timer::Skill {
                world,
                id,
                step: Step::End,
            },
        );
        return;
    };
    let told = !matches!(recall, Recall::Named { told: false, .. });
    let said = told.then(|| progress::text((text::RECALLED, table)));
    crate::places::recall_trip(s, net, world, to, said);
    s.after(
        clock,
        RECALL_IN_PORTAL,
        Timer::Skill {
            world,
            id,
            step: Step::Out,
        },
    );
}

/// A summoning's story is over: its portal appears (when the summoner keeps a place of its kind);
/// its end follows.
fn summon_portal<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some((id, skill)) = s
        .players
        .get(&world)
        .and_then(|p| p.using.as_ref())
        .map(|u| (u.req.id, u.req.skill))
    else {
        return;
    };
    crate::usable::portals::summon(s, net, world, skill, clock);
    s.after(
        clock,
        crate::usable::portals::ENDS_AFTER,
        Timer::Skill {
            world,
            id,
            step: Step::End,
        },
    );
}

/// A recall comes out of portal space at its place; its end follows.
fn recall_out<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let id = s
        .players
        .get(&world)
        .and_then(|p| p.using.as_ref())
        .map_or(0, |u| u.req.id);
    crate::places::recall_arrives(s, net, world, clock);
    s.after(
        clock,
        RECALL_ENDS_AFTER,
        Timer::Skill {
            world,
            id,
            step: Step::End,
        },
    );
}

/// A use is over: the skill's information (when it was last used, from which its recovery counts;
/// a recall's start of the wait every skill shares), and, used from rest, the player not
/// attacking, stopped and at peace; inside a run, the run's next swing. A request waiting is taken
/// up.
fn end<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let now_game = net.game_time(clock.local);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(x) = p.using.take() else {
        return;
    };
    p.last_used.insert(x.req.skill, now_game);
    let me = p.character;
    if let Some(aura) = aura_of(s, x.req.skill) {
        aura_turns_on(s, net, world, &x, aura, now_game, clock);
        crate::world::keep_player(s, world);
        take_up_waiting(s, net, world, clock);
        return;
    }
    // The effects the skill leaves on its user (a combination's link) come first; a pet it
    // summons is told of first and appears as the use's end is told.
    let mut summons = Vec::new();
    for state in &x.usage.user_effects {
        if *state == crate::effects::RECALL_PROTECTION.state {
            let skill_state = s
                .skill_table
                .skills
                .get(&x.req.skill)
                .map_or(0, |d| d.state);
            recall_protects(s, net, world, (me, skill_state), clock);
            continue;
        }
        let pet = matches!(
            s.effect_table.get(*state).map(|d| &d.kind),
            Some(crate::effect_defs::Kind::Pet { .. })
        );
        if pet {
            summons.extend(crate::pets::summon(
                s,
                net,
                world,
                x.req.skill,
                *state,
                x.spellcraft,
                clock,
            ));
            continue;
        }
        crate::effects::apply(
            s,
            net,
            world,
            *state,
            crate::effects::Cast {
                caster: me,
                spellcraft: x.spellcraft,
                pk: true,
                ..crate::effects::Cast::default()
            },
            clock,
        );
    }
    skill_info(s, net, world, &x, now_game, 0);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if x.usage.reset > 0.0 {
        let first = quals::float(&p.qualities, q::RESET_DURATION).is_none();
        if first {
            quals::set_float(&mut p.qualities, q::RESET_DURATION, x.usage.reset);
            progress::send_float(net, world, q::RESET_DURATION, x.usage.reset);
        }
        quals::set_timestamp(&mut p.qualities, q::LAST_RESET, now_game);
        progress::private(
            net,
            world,
            &ServerMessage::UpdateTimestamp(PrivateQuality {
                quality: q::LAST_RESET,
                value: now_game,
            }),
        );
    }
    let me = p.character;
    if x.alone {
        progress::private(net, world, &combat::attack_state(false));
        if x.turned {
            let stop = stop_moving(p);
            to_all_seeing(s, net, me, &stop);
        }
        combat::set_player_mode(s, net, world, combat::mode::PEACE);
    } else if let Some(run) = s.players.get_mut(&world).and_then(|p| p.attacking.as_mut()) {
        run.next_swing = clock.local.0 + NEXT_SWING_AFTER;
    }
    for pet in summons {
        crate::pets::appear(s, net, pet);
    }
    crate::world::keep_player(s, world);
    take_up_waiting(s, net, world, clock);
}

/// A city's recall protects its user as it ends: the protection, and its glow played for everyone
/// who sees it (2017: the glow after the player's place, 215 of 215).
fn recall_protects<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    from: (u64, u32),
    clock: Clock,
) {
    crate::effects::protect_with(
        s,
        net,
        world,
        crate::effects::RECALL_PROTECTION,
        from,
        clock,
    );
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.visual = p.visual.wrapping_add(1);
    let glow = ServerMessage::DoFx(ObjectEvent {
        sender: auberean_protocol::types::ObjectStamp {
            id: p.character,
            instance_stamp: p.instance,
            other_stamp: p.visual,
        },
        action: auberean_protocol::physics::Fx {
            fx: crate::effects::protection::GLOW,
            scalar: 1.0,
        },
    });
    let me = p.character;
    to_all_seeing(s, net, me, &glow);
}

/// A request that waited is taken up as the use before it ends.
fn take_up_waiting<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    if let Some(next) = s.players.get_mut(&world).and_then(|p| p.queued.take()) {
        let attacking = s.players.get(&world).is_some_and(|p| p.attacking.is_some());
        if attacking {
            if let Some(p) = s.players.get_mut(&world) {
                p.in_run = Some(next);
            }
        } else {
            starts(s, net, world, next, clock);
        }
    }
}

/// A toggled skill's use ends by turning its aura on (2017's order): the aura's first pulse and its
/// record, not attacking, the toggle on, the skill's information marked toggled, the pulse's
/// appearance, stopped (when turned) and at peace.
fn aura_turns_on<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    x: &Using,
    aura: u32,
    now_game: f64,
    clock: Clock,
) {
    let skill_state = s
        .skill_table
        .skills
        .get(&x.req.skill)
        .map_or(0, |d| d.state);
    let fx = crate::effects::aura_on(s, net, world, aura, skill_state, x.spellcraft, clock);
    if x.alone {
        progress::private(net, world, &combat::attack_state(false));
    }
    progress::private(net, world, &crate::effects::toggle_call(true, x.req.skill));
    skill_info(s, net, world, x, now_game, TOGGLED_ON);
    crate::effects::show_fxs(s, net, world, &fx);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let me = p.character;
    if x.alone {
        if x.turned {
            let stop = stop_moving(p);
            to_all_seeing(s, net, me, &stop);
        }
        combat::set_player_mode(s, net, world, combat::mode::PEACE);
    } else if let Some(run) = s.players.get_mut(&world).and_then(|p| p.attacking.as_mut()) {
        run.next_swing = clock.local.0 + NEXT_SWING_AFTER;
    }
}

/// A used skill's information: when it was last used (from which its recovery counts), with its
/// rating as the use began.
fn skill_info<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    x: &Using,
    now: f64,
    mask: u32,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let held = crate::skills::held(&starting, &p.play.skills);
    let rating = effective_rating(s, p, &held, x.req.skill);
    if let Some(mut h) = held.iter().find(|h| h.skill == x.req.skill).copied() {
        h.mask |= HAS_LAST_USED | mask;
        progress::private(
            net,
            world,
            &crate::skills::used_update(&h, rating, x.began, now),
        );
    }
}

/// A use is cut short (the player moved, stopped attacking, died): over, why, the player not
/// attacking and at peace. Before its story nothing is spent and its power-up stops; after it, it
/// counts as used (the skill's information first), its behavior stops on the player and what it
/// wields, and a player turned to its target is stopped. A missile already thrown still lands.
pub fn interrupt<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    err: u32,
    clock: Clock,
) {
    let now = net.game_time(clock.local);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(x) = p.using.take() else {
        return;
    };
    if let Some(old) = p.queued.take() {
        progress::private(net, world, &ended(old.id));
    }
    let me = p.character;
    if x.told {
        p.last_used.insert(x.req.skill, now);
        skill_info(s, net, world, &x, now, 0);
    }
    progress::private(net, world, &ended(x.req.id));
    let named = if x.told {
        aimed_at(x.aim).unwrap_or(me)
    } else {
        me
    };
    progress::private(net, world, &refused(err, named, x.req.skill));
    if !x.alone {
        return;
    }
    progress::private(net, world, &combat::attack_state(false));
    let stopping = if x.told {
        Some(if x.aim == Aim::Me {
            x.usage.behavior_self[0]
        } else {
            x.usage.behavior_other[0]
        })
    } else if x.usage.powerup > 0.0 {
        Some(x.usage.powerup_behavior)
    } else {
        None
    };
    if let Some(b) = stopping {
        if let Some(p) = s.players.get_mut(&world) {
            let stop = ServerMessage::StopBehavior(player_event(p, Word(b)));
            to_all_seeing(s, net, me, &stop);
        }
        if x.told {
            for (item, _) in wielded(s, me) {
                let stop = ServerMessage::StopBehavior(ObjectEvent {
                    sender: crate::inventory::visual(s, item),
                    action: Word(b),
                });
                to_all_seeing(s, net, me, &stop);
            }
        }
    }
    if x.told && x.turned {
        if let Some(p) = s.players.get_mut(&world) {
            let stop = stop_moving(p);
            to_all_seeing(s, net, me, &stop);
        }
    }
    combat::set_player_mode(s, net, world, combat::mode::PEACE);
}

/// The player reported a place: moving cuts short a use that is not used on the move (2017: 803
/// of 809 such uses cut by a moving report; uses on the move, 2 of 861).
pub fn moved<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let cut = s.players.get(&world).is_some_and(|p| {
        moving(p)
            && p.using
                .as_ref()
                .is_some_and(|x| x.usage.ui_flags & ui_flag::USE_WHILE_MOVING == 0)
    });
    if cut {
        interrupt(s, net, world, refusal::INTERRUPTED, clock);
    }
}

/// A transmute cuts short a use going as it is carried out, its story out or not (2017: 14 of 14,
/// six of them asked in the same instant), but a recall, whose trip is its own. An item used or a
/// trip into portal space cuts nothing by itself (2017: 1,929 uses past their story ended as they
/// would at an item's use; every cut there followed the target's death and the client's stopping).
pub fn cut_by_transmute<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    clock: Clock,
) {
    let cut = s
        .players
        .get(&world)
        .and_then(|p| p.using.as_ref())
        .map(|x| x.req.skill)
        .is_some_and(|skill| !is_recall(s, skill));
    if cut {
        interrupt(s, net, world, refusal::INTERRUPTED, clock);
    }
}

/// The client stops attacking: everything (`None`), or one target. A use not used on the move is
/// cut short: by stopping everything; by stopping its target, a use aimed at it (2017: 726 of 731
/// such uses aimed at what was stopped, 78 of 78 by stopping everything; uses on the move never).
pub fn stopped<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    target: Option<u64>,
    clock: Clock,
) {
    let cut = s.players.get(&world).is_some_and(|p| {
        p.using.as_ref().is_some_and(|x| {
            x.usage.ui_flags & ui_flag::USE_WHILE_MOVING == 0
                && target.is_none_or(|t| aimed_at(x.aim) == Some(t))
        })
    });
    if cut {
        interrupt(s, net, world, refusal::INTERRUPTED, clock);
    }
}

/// A creature attacks: its opening comes a moment later.
pub fn creature_attacked<S: Store>(s: &mut Server<S>, id: u64, clock: Clock) {
    if s.things.get(id).is_some_and(|t| t.alive()) {
        s.after(
            clock,
            OPENS_AFTER,
            Timer::Opening {
                creature: id,
                closes: false,
            },
        );
    }
}

/// A creature's opening opens: the opening effect put on it (its golden rays shown while one runs;
/// open already, it lasts from now). It closes as its record runs out (`closes` is not used).
pub fn opening<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    closes: bool,
    clock: Clock,
) {
    if closes {
        return;
    }
    crate::effects::apply_on(
        s,
        net,
        crate::effects::On::Creature(id),
        OPENING_EFFECT,
        crate::effects::Cast {
            caster: id,
            spellcraft: 1.0,
            ..crate::effects::Cast::default()
        },
        clock,
    );
}

/// A text of the skills' use (for tests and logs).
#[must_use]
pub fn fatigued() -> TextInfo {
    progress::text(text::FATIGUED)
}
