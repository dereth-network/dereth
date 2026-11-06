//! Pets: the creatures a player's skills summon to fight beside it, their orders and their end.
//!
//! **What makes what** is the world's: a `[[pet]]` row names the template, flags and fighting numbers
//! of what an effect or a contract makes, a `[[pet_limit]]` row what a skill allows. The class, the
//! lifetime and the icon are the effect's and the skill's own, what a pet can do its template's
//! skills. A pet effect with no row tells its user the summoning fails, and the operator's log why.
//!
//! **Summoned** as a summoning skill's use ends: the player is told of its new pet (its id, its
//! kind's name, the skill's icon, and its data: no time left in the world, its class, its flags),
//! and the pet appears two meters from its owner, naming it as its summoner, moving at 1.75 times
//! the faster of its kind's own velocity scale and its owner's. Its level is its owner's rating in
//! the skill; its vigor twenty times that, its health its kind's at that level. It is of its
//! owner's kingdom (an owner in none: the neutral one), in its description and its physics type,
//! and passes through what its owner would where it stands: its owner's movement ethereality for
//! the pet's own landblock, with a bit of its own; walking into a landblock of another status, it
//! tells everyone who knows it its new one. A summoning skill
//! is refused (the client writes "You have too many pets to use that skill.") to an owner who has
//! as many live pets of a class as the skill allows; a pet counts toward every class it shares a bit
//! with, and others do not count. Aimed at a creature, its pet appears beside it and goes after it.
//!
//! **Effects on it**: a skill aimed at pets lands on its user's own, of the class it names, and never
//! harms it. What it leaves there can open one of the pet's attacks and bar another, as each attack
//! says. A pet whose template's skills include no attack takes no order to attack and fights nothing.
//!
//! **Orders** (the pet window's, and `/pet`): a mode is echoed back, pet and mode, and kept; a
//! name is kept in the written form of names and shown as "Name (kind)", to the owner as the pet's
//! name and to everyone as the pet's own (nothing new is shown when the name is the one it has);
//! an attack sends every pet after the target unless the target is more than 32 m from the owner
//! ("You are too far away from the target to order your pets to attack it."), with no answer; a
//! return calls every pet back, dropping its fight until it is with its owner again; a death order
//! (one pet, or all) kills the pet. Attacks and returns run up a debt of a second each that drains
//! a second a second: one ordered with more than five seconds owed is refused ("You have executed
//! too many pet commands recently. ...", with the debt past five as the wait).
//!
//! **Behavior**, by the pet's mode (a new pet supports its owner): every three seconds a pet not
//! fighting plans its way to three meters from where its owner then stands (so it trails a running
//! owner), and one more than 100 m away is put beside its owner; a supporting pet takes on what its
//! owner attacks, a protective one what goes after its owner, a self-defending one what strikes
//! it. A pet that goes into a fight while still on its way to its owner is told its kind's own
//! speed as it does. In a fight it closes to its attacks' reach and attacks about once a second. A
//! creature a pet strikes fights the pet: at once when it fought nothing, and turning from another
//! three times in five.
//!
//! **The end**: killed, ordered to die, or with its owner's death (in the owner's death tick), the
//! pet is taken off its owner's list and falls as a creature does, in one tick (its physics type
//! as it dies its own, kingdom and all, with the dead bit); its corpse comes a
//! second and a half later and its body leaves three seconds after that. An owner leaving the
//! world takes its pets out of the world with it (nothing more sent to the owner). A pet goes with
//! its owner through portals, except one that stays where it is made.

use std::collections::{BTreeMap, HashMap};

use auberean_protocol::physics::ObjectPosition;
use auberean_protocol::physics::Scale as ScaleEvent;
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::text::StringInfo;
use auberean_protocol::types::ObjectStamp;
use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::native_object::TextInfo;
pub use yalain_content::{PetHire, PetKind, PetTends, PetWhom};
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::calls::Value;
use crate::graph::{Builder, Slot};
use crate::missiles::AttackSkill;
use crate::rules::Outcome;
use crate::things::{Creature, Kind, Mode, Thing};
use crate::{progress, session, Clock, Server};

/// The pets' calls.
pub mod function {
    /// The server's: a pet added to the player's list, one removed, its mode, its name.
    pub const ADD: u32 = 0x8200_001B;
    pub const REMOVE: u32 = 0x8200_001A;
    pub const UPDATE_MODE: u32 = 0x8200_0035;
    pub const UPDATE_NAME: u32 = 0x8200_0034;
    /// The client's: attack a target, come back, take a mode, die (0: all pets), take a name.
    pub const ATTACK: u32 = 0x8212_0041;
    pub const RETURN: u32 = 0x8212_0038;
    pub const MODE: u32 = 0x8212_0037;
    pub const DIE: u32 = 0x8212_003B;
    pub const NAME: u32 = 0x8212_003A;
}

/// The texts of pets (string id, table).
pub mod text {
    pub const TOO_FAR: (u32, u32) = (0x795F_14C6, 0x2500_000B);
    pub const TOO_MANY: (u32, u32) = (0x0B95_CBF8, 0x2500_000B);
    /// The seconds to wait, a variable of the too-many text (a number shown with one digit).
    pub const WAIT_VAR: u32 = 0xA926_6C50;
    pub const WAIT_DIGITS: u16 = 1;
    /// "Name (kind)": the chosen name and the kind's name.
    pub const NAMED: (u32, u32) = (0xBA35_9CD7, 0x2500_0011);
    pub const NAME_VAR: u32 = 0x9274_24E8;
    pub const KIND_VAR: u32 = 0xAC50_A4E8;
    /// "The summoning fails."
    pub const SUMMONING_FAILS: (u32, u32) = (0xA5AC_6CBF, 0x2500_0CE5);
    /// "That pet name is too long."
    pub const NAME_TOO_LONG: (u32, u32) = (0x9353_92A2, 0x2500_000B);
    /// "That pet name is not allowed."
    pub const NAME_NOT_ALLOWED: (u32, u32) = (0x3C48_5626, 0x2500_000B);
}

/// Why a summoning skill is refused: its user has as many pets of the class as it allows.
pub const TOO_MANY_PETS: u32 = 30_042;

/// The package of a pet's data, and the time left in the world it carries (none: -1).
pub const PET_DATA: u16 = 0x12C;
pub const NO_TIME_LEFT: f64 = -1.0;

/// The visual quality of an object's name.
pub const NAME_QUALITY: u32 = 256;

/// A pet's velocity scale over the faster of its kind's and its owner's (2017: 1.75 times; the
/// Lasher, whose kind's is 2, 3.5 on every object made).
pub const VELOCITY_FACTOR: f32 = 1.75;
/// A pet's acceleration scale over its velocity scale (2017: 0.857, 22,006 of 22,011 moves).
pub const ACCELERATION_FACTOR: f32 = 6.0 / 7.0;

/// How far from its owner a pet appears, in meters (2017: a median 2.0 for every kind).
pub const APPEARS_AWAY: f32 = 2.0;

/// The template property of a kind's own velocity scale.
pub const KIND_VELOCITY: &str = "Physics_VelScale";

/// The pets of a world with no `[[pet]]` rows: a world database written before them names none, and
/// these keep its summoning skills working. A row of the same effect replaces one of these.
#[must_use]
pub fn fallback_kinds() -> Vec<PetKind> {
    let kinds = [
        // The Lasher.
        PetKind::new(0x7000_0805, 0x4700_0846, 1096, &LASHER_HEALTH).hits_for(0.075, 0.45, 0.78),
        // The Shreth.
        PetKind::new(0x7000_092C, 0x4700_0863, 1096, &[]).hits_for(0.047, 0.35, 0.77),
        // The Sand Warrior, Sand Fiend and Sand Fury.
        PetKind::new(0x7000_082D, 0x4700_084B, 1096, &[]).hits_for(0.08, 0.35, 0.84),
        PetKind::new(0x7000_082C, 0x4700_084A, 1096, &[]).hits_for(0.08, 0.35, 0.84),
        PetKind::new(0x7000_082E, 0x4700_084C, 1096, &[]).hits_for(0.08, 0.35, 0.84),
        // The Volcanic Rift: it stays where it is made, its fire falling on all near where it
        // lands; it leaves no corpse.
        PetKind::new(0x7000_0830, 0x4700_084E, 1184, &[])
            .hits_for(0.035, 0.5, 0.73)
            .staying()
            .landing_on_all_within(10.0)
            .corpse(None),
        // The Large and Small Turrets: they stay, cannot be set down indoors.
        PetKind::new(0x7000_08B7, 0x4700_0851, 3128, &[])
            .hits_for(0.07, 0.5, 0.75)
            .staying()
            .outdoors_only()
            .corpse(Some(2.8))
            .most_of_its_class(2),
        PetKind::new(0x7000_08BD, 0x4700_0853, 3128, &[])
            .hits_for(0.05, 0.5, 0.75)
            .staying()
            .outdoors_only()
            .corpse(Some(2.8))
            .most_of_its_class(2),
        // The Taunt Orb (its strikes taunt and do no harm) and Axe Orb (four hits a swing).
        PetKind::new(0x7000_1F1F, 0x4700_083E, 1096, &[])
            .taunting()
            .corpse(Some(1.1)),
        PetKind::new(0x7000_1F1D, 0x4700_083D, 1096, &[])
            .hits_for(0.08, 0.35, 0.8)
            .hits_a_swing(4),
        // Living Armor.
        PetKind::new(0x7000_1F22, 0x4700_0840, 1096, &[]).hits_for(0.04, 0.35, 0.8),
        // The mercenaries, known by their contracts: see [`mercenary`].
        PetKind::new(mercenary::CRAFTER, 0x4700_3453, 1096, &[])
            .hits_for(0.05, 0.1, 0.8)
            .hired(mercenary::HIRE),
        PetKind::new(mercenary::KEEPER, 0x4700_3455, 1096, &[])
            .hits_for(0.05, 0.1, 0.8)
            .hired(mercenary::HIRE)
            .tending(mercenary::tends(PetWhom::OwnersPets)),
        PetKind::new(mercenary::MEDIC, 0x4700_3457, 1096, &[])
            .hits_for(0.05, 0.1, 0.8)
            .hired(mercenary::HIRE)
            .tending(mercenary::tends(PetWhom::Owner)),
    ];
    kinds
        .into_iter()
        .map(|k| match k.most_of_class {
            Some(_) => k,
            None => k.most_of_its_class(1),
        })
        .collect()
}

/// Mercenaries, hired by contract. The numbers here are this server's own: the summons they would
/// run are in no data the server holds, nothing recorded shows one, and each is meant to be changed
/// when something better is known. A contract hires at once as it is bought, dismisses the mercenary
/// before it (one at a time, as its text says) and is used up. The mercenary follows and fights as a
/// pet does, on its template's own numbers; a medic tends its owner, a keeper its owner's other pets.
/// It ends after an hour, with its owner's death or with its owner leaving the world.
pub mod mercenary {
    use super::{PetHire, PetTends, PetWhom};

    /// The contracts' templates, which also name their kinds.
    pub const CRAFTER: u32 = 0x4700_18D1;
    pub const KEEPER: u32 = 0x4700_18D2;
    pub const MEDIC: u32 = 0x4700_18D3;
    /// Its class of pet, apart from every class a skill summons.
    pub const CLASS: u32 = 0x10_0000;
    /// How long it stays, in seconds.
    pub const SECONDS: f64 = 3600.0;
    /// Its level, its template's, and the spellcraft it tends at.
    pub const LEVEL: f32 = 20.0;
    /// How often it may tend, in seconds, the share of whole health below which it does, and the
    /// effect it tends with (a healer's plain heal).
    pub const TENDS_EVERY: f64 = 30.0;
    pub const TENDS_BELOW: f32 = 0.75;
    pub const TEND_EFFECT: u32 = 0x7000_0772;
    /// A hiring's terms.
    pub const HIRE: PetHire = PetHire {
        class: CLASS,
        seconds: SECONDS,
        level: LEVEL,
    };
    /// A mercenary's tending of someone.
    #[must_use]
    pub const fn tends(whom: PetWhom) -> PetTends {
        PetTends {
            whom,
            effect: TEND_EFFECT,
            every: TENDS_EVERY,
            below: TENDS_BELOW,
        }
    }
}

/// The Lasher's greatest health at its summoner's ratings (2017's: 29, 52, 53 and 54).
pub const LASHER_HEALTH: [(i32, i32); 4] = [(29, 663), (52, 1762), (53, 1813), (54, 1863)];

/// A pet's vigor per level of its summoner's rating (2017: exactly 20 times, 4 of 4).
pub const VIGOR_PER_LEVEL: i32 = 20;

/// How many pets an owner may have at once whatever its skills allow, so a skill that allows any
/// number stays within reason. This server's own: 2017 shows no owner near it.
pub const MOST_PETS: usize = 16;

/// The flags of what a pet does by itself.
pub mod flag {
    /// It takes on what goes after its owner.
    pub const DEFEND_MASTER: u32 = 0x4;
    /// It takes on what its owner attacks.
    pub const SUPPORT_MASTER: u32 = 0x8;
    /// It fights back what strikes it.
    pub const SELF_DEFENDING: u32 = 0x10;
    /// It takes on, unbidden, the nearest creature within its reach.
    pub const AGGRESSIVE: u32 = 0x20;
    /// It is put beside its owner when too far.
    pub const TELEPORT_TO_MASTER: u32 = 0x40;
    /// The flags a mode sets.
    pub const BEHAVIOR: u32 = 0x703C;
}

/// The behavior flags of a mode (the client's own table of modes).
#[must_use]
pub fn mode_flags(mode: u32) -> u32 {
    match mode {
        0x4000_0001 => 0x4,
        0x4000_0002 => 0x8,
        0x4000_0003 => 0xC,
        0x4000_0004 => 0x14,
        0x4000_0006 => 0x18,
        0x20 => 0x38,
        0x1000 => 0x1030,
        0x2000 => 0x2030,
        0x4000 => 0x4000,
        m if m & 0x20 != 0 => 0x20,
        _ => 0,
    }
}

/// What a pet has been told to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// Keep near its owner and take on what its mode has it take on.
    Follow,
    /// Come back to its owner, leaving any fight, until it is with it again.
    Return,
}

/// One pet.
#[derive(Debug, Clone, PartialEq)]
pub struct Pet {
    pub id: u64,
    /// Its owner (a character id).
    pub owner: u64,
    pub template: u32,
    pub class: u32,
    /// Its flags: what it does by itself (its mode's), and the rest its kind's.
    pub flags: u32,
    /// Its mode, as last set (0: never set).
    pub mode: u32,
    /// The name its owner gave it, in the written form.
    pub name: Option<String>,
    pub order: Order,
    /// When it next plans its way to its owner (world seconds), and where it is going.
    pub next_plan: f64,
    pub goal: Option<StoredPosition>,
    /// Its kind's pet effect.
    pub effect: u32,
    /// What its owner last hurt, which a supporting pet takes on.
    pub assist: Option<u64>,
    /// When it may next tend.
    pub next_tend: f64,
}

/// Every pet in the world, each owner's debt of orders, and the kinds of pet the world makes.
#[derive(Debug, Clone, PartialEq)]
pub struct Pets {
    pub by_id: BTreeMap<u64, Pet>,
    /// Each owner's debt of orders in seconds, as of a world second.
    pub owed: HashMap<u64, (f64, f64)>,
    /// Pets told of whose summoning failed, by their owners (taken off the owner's list as the use
    /// ends).
    pub failed: BTreeMap<u64, u64>,
    /// What each pet not yet out goes after as it appears.
    pub goes_after: BTreeMap<u64, u64>,
    /// The kinds of pet, by the effect (or the contract's template) that names them.
    pub kinds: BTreeMap<u32, PetKind>,
    /// What each summoning skill allows its user: as many as this of pets of these classes. A pet
    /// counts toward every listed class it shares a bit of class with.
    pub limits: BTreeMap<u32, Vec<(usize, u32)>>,
}

impl Default for Pets {
    fn default() -> Self {
        Self {
            by_id: BTreeMap::new(),
            owed: HashMap::new(),
            failed: BTreeMap::new(),
            goes_after: BTreeMap::new(),
            kinds: fallback_kinds()
                .into_iter()
                .map(|k| (k.effect, k))
                .collect(),
            limits: BTreeMap::new(),
        }
    }
}

impl Pets {
    /// A world's rows take their place among the kinds and limits it knows.
    pub fn read_rows(
        &mut self,
        kinds: impl IntoIterator<Item = PetKind>,
        limits: impl IntoIterator<Item = (u32, Vec<(usize, u32)>)>,
    ) {
        self.kinds.extend(kinds.into_iter().map(|k| (k.effect, k)));
        self.limits.extend(limits);
    }

    /// The kind of pet an effect summons, when the world knows it.
    #[must_use]
    pub fn kind_of(&self, effect: u32) -> Option<&PetKind> {
        self.kinds.get(&effect)
    }

    /// Is an item of this template a contract that hires a mercenary?
    #[must_use]
    pub fn is_contract(&self, template: u32) -> bool {
        self.kinds.get(&template).is_some_and(|k| k.hire.is_some())
    }

    /// The pets of an owner, in the order they were summoned.
    #[must_use]
    pub fn of(&self, owner: u64) -> Vec<u64> {
        self.by_id
            .values()
            .filter(|p| p.owner == owner)
            .map(|p| p.id)
            .collect()
    }

    /// Is this object a pet?
    #[must_use]
    pub fn is_pet(&self, id: u64) -> bool {
        self.by_id.contains_key(&id)
    }

    /// How many live pets an owner has that share a bit of these classes.
    #[must_use]
    pub fn of_class(&self, owner: u64, classes: u32) -> usize {
        self.by_id
            .values()
            .filter(|p| p.owner == owner && p.class & classes != 0)
            .count()
    }
}

/// A pet's data: no time left in the world, its class, its flags.
fn pet_data(class: u32, flags: u32) -> Arg {
    let mut b = Builder::new();
    b.object(
        PET_DATA,
        &[Slot::Time(NO_TIME_LEFT), Slot::Int(class), Slot::Int(flags)],
    );
    Arg::Object(b.graph())
}

/// The call telling a player of a new pet: its id, its name, its icon, its data.
#[must_use]
pub fn added(pet: u64, name: TextInfo, icon: u32, class: u32, flags: u32) -> ServerMessage {
    session::call(
        function::ADD,
        &[
            pet_data(class, flags),
            Arg::Int(icon),
            crate::chat::text_arg(name),
            Arg::LongInt(pet),
        ],
    )
    .expect("a pet packs")
}

/// The call taking a pet off its owner's list.
#[must_use]
pub fn removed(pet: u64) -> ServerMessage {
    session::call(function::REMOVE, &[Arg::LongInt(pet)]).expect("a pet packs")
}

/// The call telling a player its pet's mode.
#[must_use]
pub fn mode_update(pet: u64, mode: u32) -> ServerMessage {
    session::call(function::UPDATE_MODE, &[Arg::Int(mode), Arg::LongInt(pet)]).expect("packs")
}

/// The call telling a player its pet's name.
#[must_use]
pub fn name_update(pet: u64, name: TextInfo) -> ServerMessage {
    session::call(
        function::UPDATE_NAME,
        &[crate::chat::text_arg(name), Arg::LongInt(pet)],
    )
    .expect("a name packs")
}

/// A pet's name as shown: "Name (kind)", the chosen name as it is written, the kind's name.
#[must_use]
pub fn shown_name(chosen: &str, kind: (u32, u32)) -> TextInfo {
    let literal = TextInfo {
        has_literal: 1,
        literal: Some(chosen.encode_utf16().collect()),
        ..TextInfo::default()
    };
    progress::text_with(
        text::NAMED,
        vec![
            progress::nested(text::KIND_VAR, progress::text(kind)),
            progress::nested(text::NAME_VAR, literal),
        ],
    )
}

/// The written form of a name a player gives its pet (the written form of names: trimmed, each
/// letter a capital after anything but a letter and small after a letter).
#[must_use]
pub fn written(typed: &str) -> String {
    let units: Vec<u16> = typed.encode_utf16().collect();
    let (w, _) = auberean_text::names::format_for_audience(&units);
    String::from_utf16_lossy(&w)
}

/// A string reference as an object's name carries it (its nested references and literals).
#[must_use]
pub fn string_info(t: &TextInfo) -> StringInfo {
    use auberean_protocol::text::{Variable, VariableValue, WideString};
    use auberean_script::native_object::VariableValue as V;
    StringInfo {
        string_id: t.string_id,
        table_id: t.table,
        variable_count: i16::try_from(t.count).unwrap_or(0),
        has_literal: t.has_literal,
        literal: t.literal.clone().map(WideString),
        variables: t
            .variables
            .iter()
            .filter_map(|v| {
                let value = match &v.value {
                    V::Nested(n) => VariableValue::Nested(Box::new(string_info(n))),
                    V::Word(w) => VariableValue::Word(*w),
                    V::Long(l) => VariableValue::Long(*l),
                    _ => return None,
                };
                Some(Variable {
                    id: v.id,
                    kind: v.kind,
                    value,
                })
            })
            .collect(),
    }
}

/// The world connection of a character in the world.
fn conn_of<S: Store>(s: &Server<S>, character: u64) -> Option<ConnId> {
    s.players
        .iter()
        .find(|(_, p)| p.character == character)
        .map(|(c, _)| *c)
}

/// Is a skill's use refused because its user has as many pets of a class as it allows? A skill the
/// world lists goes by that; one it doesn't, by what each kind it summons allows of its own class.
#[must_use]
pub fn too_many<S: Store>(s: &Server<S>, owner: u64, skill: u32, effects: &[u32]) -> bool {
    let summons: Vec<(u32, u32)> = effects
        .iter()
        .filter_map(|e| match s.effect_table.get(*e).map(|d| &d.kind) {
            Some(crate::effect_defs::Kind::Pet { class, .. }) => Some((*e, *class)),
            _ => None,
        })
        .collect();
    if summons.is_empty() {
        return false;
    }
    let limited = match s.pets.limits.get(&skill) {
        Some(limits) => limits
            .iter()
            .any(|(most, classes)| s.pets.of_class(owner, *classes) >= *most),
        None => summons.iter().any(|(e, class)| {
            s.pets
                .kind_of(*e)
                .and_then(|k| k.most_of_class)
                .is_some_and(|most| s.pets.of_class(owner, *class) >= most)
        }),
    };
    limited || s.pets.of(owner).len() >= MOST_PETS
}

/// The skills a player can train that summon what the world has no kind of pet for.
#[must_use]
pub fn skills_with_no_kind<S: Store>(s: &Server<S>) -> Vec<u32> {
    let trainable = s
        .skill_table
        .skills
        .iter()
        .filter(|(_, d)| d.flags & 1 != 0);
    trainable
        .filter(|(_, d)| {
            d.usage.as_ref().is_some_and(|u| {
                u.user_effects.iter().chain(&u.target_effects).any(|e| {
                    matches!(
                        s.effect_table.get(*e).map(|d| &d.kind),
                        Some(crate::effect_defs::Kind::Pet { .. })
                    ) && s.pets.kind_of(*e).is_none()
                })
            })
        })
        .map(|(id, _)| *id)
        .collect()
}

/// A summoning: of which skill and effect, at what rating.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summoning {
    pub skill: u32,
    pub effect: u32,
    pub rating: f32,
    /// Where it is made (none: before its owner), and what it goes after at once.
    pub at: Option<StoredPosition>,
    pub after: Option<u64>,
}

/// A summoning skill's use ends with its pet effect: the player is told of each new pet, which
/// appears with [`appear`] once the use's end is told. A summoning that can't make what it names
/// is told as one that fails. Returns the pets.
pub fn summon<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    what: Summoning,
    clock: Clock,
) -> Vec<u64> {
    let (class, seconds) = match s.effect_table.get(what.effect).map(|d| d.kind.clone()) {
        Some(crate::effect_defs::Kind::Pet { class, lifetime }) => (
            class,
            lifetime.at(what.rating).filter(|x| *x > 0.0).map(f64::from),
        ),
        _ => return Vec::new(),
    };
    let icon = s.skill_table.skills.get(&what.skill).map_or(0, |d| d.icon);
    let Some(kind) = s.pets.kind_of(what.effect).cloned() else {
        tracing::warn!(
            skill = what.skill,
            effect = format_args!("{:08X}", what.effect),
            "a summoning names a pet this world has no [[pet]] row for: it fails"
        );
        s.unhandled
            .note(format!("a pet effect {:08X} with no row", what.effect));
        return vec![fails(s, net, world, (0, 0), (icon, class, 0))];
    };
    let spot = what.at.or_else(|| {
        (kind.made > 1)
            .then(|| s.players.get(&world).map(|p| beside(&p.position)))
            .flatten()
    });
    let mut made = Vec::new();
    for n in 0..kind.made {
        let at = spot.map(|at| around(&at, n, kind.made));
        let about = (class, icon, seconds);
        let pet = summon_kind(s, net, world, &kind, about, (what.rating, at), clock);
        made.extend(pet);
    }
    if let Some(target) = what.after {
        s.pets.goes_after.extend(made.iter().map(|p| (*p, target)));
    }
    made
}

/// Told of, then "The summoning fails.", and taken off the list as the use ends (2017: 8 of 8).
fn fails<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    name: (u32, u32),
    (icon, class, flags): (u32, u32, u32),
) -> u64 {
    let owner = s.players.get(&world).map_or(0, |p| p.character);
    let id = s.things.new_id();
    progress::private(
        net,
        world,
        &added(id, progress::text(name), icon, class, flags),
    );
    progress::private(
        net,
        world,
        &crate::chat::show_text(1, progress::text(text::SUMMONING_FAILS)),
    );
    s.pets.failed.insert(id, owner);
    id
}

/// One of `of` places about a spot (the spot itself when it is the only one).
fn around(at: &StoredPosition, n: u32, of: u32) -> StoredPosition {
    if of < 2 {
        return *at;
    }
    #[allow(clippy::cast_precision_loss)]
    let a = (n as f32) * std::f32::consts::TAU / (of as f32);
    let (dx, dy) = (
        MADE_AROUND * dereth_primitives::num::math::sinf(a),
        MADE_AROUND * dereth_primitives::num::math::cosf(a),
    );
    moved(at, dx, dy)
}

/// A spot so far off another, in its cell or the outdoor cell it falls in.
fn moved(at: &StoredPosition, dx: f32, dy: f32) -> StoredPosition {
    let (cell, x, y) = if crate::presence::indoors(at.cell) {
        (at.cell, at.origin[0] + dx, at.origin[1] + dy)
    } else {
        crate::spawn::outdoor(at.cell >> 16, at.origin[0] + dx, at.origin[1] + dy)
    };
    StoredPosition {
        cell,
        origin: [x, y, at.origin[2]],
        rotation: at.rotation,
    }
}

/// How far from their spot pets made together stand, in meters (their effect's own number).
pub const MADE_AROUND: f32 = 2.0;

/// A pet of a kind appears for a player: of a class, with an icon, for so many seconds when it does
/// not stay until it is killed. Returns the pet.
fn summon_kind<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    kind: &PetKind,
    (class, icon, seconds): (u32, u32, Option<f64>),
    (rating, place): (f32, Option<StoredPosition>),
    clock: Clock,
) -> Option<u64> {
    let (owner, owner_at) = s.players.get(&world).map(|p| (p.character, p.position))?;
    let name = s
        .catalog
        .get(kind.template)
        .and_then(|t| t.description.name)
        .unwrap_or_default();
    let level = dereth_primitives::num::to_i32(rating.round()).max(1);
    let made = s.catalog.get(kind.template).cloned().and_then(|template| {
        let mut props = template.props.clone();
        props.set(
            yalain_content::props::creature_property::LEVEL,
            yalain_content::Prop::Int(i64::from(level)),
        );
        let stats = yalain_content::CreatureStats::from_props(&props)?;
        Some((template, props, stats))
    });
    let at = place.unwrap_or_else(|| beside(&owner_at));
    if made.is_none() {
        tracing::warn!(
            template = format_args!("{:08X}", kind.template),
            "a summoning's template is not one the world describes as a creature: it fails"
        );
    }
    let Some((template, props, mut stats)) =
        made.filter(|_| !kind.outdoors_only || !crate::presence::indoors(at.cell))
    else {
        return Some(fails(s, net, world, name, (icon, class, kind.flags)));
    };
    if let Some(h) = kind.health_at(level) {
        stats.max_health = h.max(stats.max_health);
    }
    stats.max_vigor = VIGOR_PER_LEVEL * level;
    let own = props.float(KIND_VELOCITY).unwrap_or(1.0);
    let velocity = VELOCITY_FACTOR * own.max(crate::effects::player_speed(s, world).0);
    let id = s.things.new_id();
    progress::private(
        net,
        world,
        &added(id, progress::text(name), icon, class, kind.flags),
    );
    let now = clock.local.0;
    let mut t = Thing::of(id, &template, Kind::Creature);
    t.at = Some(at);
    if let Some((k, ethereal)) = of_owners_kingdom(s, owner, at.cell) {
        t.kingdom = Some(k);
        t.description.numbers[PHYSICS] =
            t.description.numbers[PHYSICS] & !KINGDOM_BITS | crate::kingdoms::bit(k);
        t.description.numbers[ETHEREAL] = ethereal;
    }
    t.heading_motion = u32::from(crate::world::heading_of_rotation(owner_at.rotation)) << 24
        | crate::world::IDLE_MOTION;
    t.max_health = stats.max_health;
    t.health = stats.max_health;
    t.max_vigor = stats.max_vigor;
    t.vigor = stats.max_vigor;
    t.fresh = true;
    t.summoner = Some(owner);
    t.velocity_scale = Some(velocity);
    t.creature = Some(Creature {
        stats,
        camp: None,
        home: at,
        mode: Mode::Idle,
        target: None,
        next_attack: now,
        next_wander: f64::INFINITY,
        next_idle: f64::INFINITY,
        moving_to: None,
        wander_to: None,
        blocked: 0,
        way: crate::ai::Way::default(),
        damage_by: Vec::new(),
        taken_by: Vec::new(),
        unreached: Vec::new(),
        ranged: false,
        closing: None,
        barred: Vec::new(),
        next_look: f64::INFINITY,
        lost_since: None,
        joining: None,
        opening: None,
        harm: Vec::new(),
        taunts: Vec::new(),
        considered: f64::NEG_INFINITY,
        target_since: now,
        kin_helped: None,
        weapons: Vec::new(),
        weapon_count: 1,
        patrol: None,
        actions: 0,
    });
    s.things.insert(t);
    s.pets.by_id.insert(
        id,
        Pet {
            id,
            owner,
            template: kind.template,
            class,
            flags: kind.flags,
            mode: 0,
            name: None,
            order: Order::Follow,
            next_plan: now + PLAN_EVERY,
            goal: None,
            effect: kind.effect,
            assist: None,
            next_tend: now,
        },
    );
    if let Some(seconds) = seconds {
        s.after(clock, seconds, crate::Timer::PetExpires { pet: id });
    }
    Some(id)
}

/// May a player hire on this contract now? Its mercenary's template must make a pet, and the player
/// be in the world, alive and out of portal space.
#[must_use]
pub fn may_hire<S: Store>(s: &Server<S>, world: ConnId, contract: u32) -> bool {
    let makes = s
        .pets
        .kind_of(contract)
        .filter(|k| k.hire.is_some())
        .and_then(|k| s.catalog.get(k.template))
        .is_some_and(|t| yalain_content::CreatureStats::from_props(&t.props).is_some());
    makes
        && s.players
            .get(&world)
            .is_some_and(|p| !crate::combat::is_dead(p) && p.dying.is_none() && p.trip.is_none())
}

/// A contract in a player's pack is carried out: the mercenary appears, the one before it goes, and
/// the contract is used up. When it can't be, the contract stays and the player is told: nothing
/// is lost, and using the contract tries again. Returns whether it hired.
pub fn hire<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    contract: u32,
    clock: Clock,
) -> bool {
    let kind = may_hire(s, world, contract)
        .then(|| s.pets.kind_of(contract).cloned())
        .flatten();
    let icon = s
        .catalog
        .get(contract)
        .and_then(|t| t.props.word("Icon"))
        .unwrap_or(0);
    let hired = kind.and_then(|k| {
        let terms = k.hire?;
        let about = (terms.class, icon, Some(terms.seconds));
        summon_kind(s, net, world, &k, about, (terms.level, None), clock)
            .map(|id| (id, terms.class))
    });
    let Some((hired, class)) = hired else {
        progress::private(
            net,
            world,
            &crate::chat::show_text(1, progress::text(text::SUMMONING_FAILS)),
        );
        return false;
    };
    let owner = s.players.get(&world).map_or(0, |p| p.character);
    for old in s.pets.of(owner) {
        let is_hired = s.pets.by_id.get(&old).is_some_and(|p| p.class == class);
        if old != hired && is_hired {
            expires(s, net, old);
        }
    }
    appear(s, net, hired);
    crate::inventory::take_from_pack(s, net, world, contract);
    true
}

/// A pet that tends does so, when its time has come and someone it tends needs it, at its level.
fn tend<S: Store>(s: &mut Server<S>, net: &mut ServerNet, pet: &Pet, clock: Clock) {
    let Some(tends) = s.pets.kind_of(pet.effect).and_then(|k| k.tends) else {
        return;
    };
    let now = clock.local.0;
    if now < pet.next_tend {
        return;
    }
    let low = |n: i32, of: i32| {
        #[allow(clippy::cast_precision_loss)]
        let share = n as f32 / of.max(1) as f32;
        share < tends.below
    };
    let on = match tends.whom {
        PetWhom::Owner => s
            .players
            .iter()
            .find(|(_, p)| p.character == pet.owner && !crate::combat::is_dead(p))
            .filter(|(_, p)| {
                low(
                    crate::quals::int(&p.qualities, crate::player::quality::HEALTH).unwrap_or(0),
                    crate::quals::int(&p.qualities, crate::player::quality::HEALTH_MAX)
                        .unwrap_or(0),
                )
            })
            .map(|(c, _)| crate::effects::On::Player(*c)),
        PetWhom::OwnersPets => s
            .pets
            .of(pet.owner)
            .into_iter()
            .filter(|id| *id != pet.id)
            .find(|id| {
                s.things
                    .get(*id)
                    .is_some_and(|t| t.alive() && low(t.health, t.max_health))
            })
            .map(crate::effects::On::Creature),
    };
    let Some(on) = on else {
        return;
    };
    if let Some(p) = s.pets.by_id.get_mut(&pet.id) {
        p.next_tend = now + tends.every;
    }
    #[allow(clippy::cast_precision_loss)]
    let spellcraft = s
        .things
        .get(pet.id)
        .and_then(|t| t.creature.as_ref())
        .map_or(1.0, |c| c.stats.level as f32);
    crate::effects::apply_on(
        s,
        net,
        on,
        tends.effect,
        crate::effects::Cast {
            caster: pet.owner,
            spellcraft,
            ..crate::effects::Cast::default()
        },
        clock,
    );
}

/// A pet's description's numbers its kingdom sets: its physics type and its movement ethereality
/// (their low words); the kingdoms' bits in them.
const PHYSICS: usize = 0;
const ETHEREAL: usize = 2;
const KINGDOM_BITS: i32 = 0x3C00_0000;

/// The bit a pet's movement ethereality carries beside its owner's.
pub const ETHEREAL_BIT: i32 = 0x4000_0000;

/// What a pet takes of its owner's kingdom where it stands in `cell`: the kingdom (the owner's as
/// it counts), and the movement ethereality, the owner's for the status of the pet's landblock
/// with the owner's permissions (a pet is never protected: an owner's protection leaves its pets
/// as they were), with the pet's own bit. `None` when the owner is not in the world.
#[must_use]
pub fn of_owners_kingdom<S: Store>(s: &Server<S>, owner: u64, cell: u32) -> Option<(u32, i32)> {
    let p = s.players.values().find(|p| p.character == owner)?;
    let q = crate::quals::read(&p.qualities);
    let (allow, _) = crate::kingdoms::permissions_of(&q);
    let area = crate::kingdoms::status_at(&s.content.conflict, cell);
    let status = crate::kingdoms::status_with(area, allow, false);
    let k = crate::kingdoms::counted(crate::kingdoms::of_qualities(&q));
    Some((
        k,
        crate::kingdoms::ethereality(status, Some(k)) | ETHEREAL_BIT,
    ))
}

/// A pet has moved into another cell: when the landblock it stands in now calls for another
/// movement ethereality, it takes it, and every client that knows the pet is told.
pub fn settle_ethereality<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let Some(owner) = s.pets.by_id.get(&id).map(|p| p.owner) else {
        return;
    };
    let Some(cell) = s.things.get(id).and_then(|t| t.at).map(|a| a.cell) else {
        return;
    };
    let Some((_, want)) = of_owners_kingdom(s, owner, cell) else {
        return;
    };
    let Some(t) = s.things.get_mut(id) else {
        return;
    };
    if t.description.numbers[ETHEREAL] == want {
        return;
    }
    t.description.numbers[ETHEREAL] = want;
    let m = ServerMessage::UpdateIntVisual(VisualQuality {
        object: crate::inventory::visual(s, id),
        quality: crate::kingdoms::quality::ETHEREALITY,
        value: want,
    });
    crate::combat::to_all_seeing(s, net, id, &m);
}

/// A pet's lifetime is over: taken off its owner's list and out of the world at once, with no
/// fall and no corpse (2017: the rift at 60.0 s, 50 of 77; Living Armor at 1,200.0 s).
pub fn expires<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let Some(pet) = s.pets.by_id.remove(&id) else {
        return;
    };
    if let Some(world) = conn_of(s, pet.owner) {
        progress::private(net, world, &removed(id));
    }
    crate::death::vanish(s, net, id);
}

/// A place two meters in front of a player (2017: every kind, 107 of 112 Lasher creates more than
/// 1.5 m ahead along the owner's heading, none behind).
fn beside(at: &StoredPosition) -> StoredPosition {
    let h = crate::world::heading_of_rotation(at.rotation);
    let a = (f32::from(h) * 360.0 / 255.0).to_radians();
    moved(
        at,
        APPEARS_AWAY * dereth_primitives::num::math::sinf(a),
        APPEARS_AWAY * dereth_primitives::num::math::cosf(a),
    )
}

/// A summoned pet appears to those who see it, once its summoner's use has ended, and goes after
/// what it was summoned at; one whose summoning failed is taken off its owner's list instead.
pub fn appear<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    if let Some(owner) = s.pets.failed.remove(&id) {
        if let Some(world) = conn_of(s, owner) {
            progress::private(net, world, &removed(id));
        }
        return;
    }
    crate::presence::thing_appears(s, net, id);
    if let Some(target) = s.pets.goes_after.remove(&id) {
        take_on(s, net, id, target);
    }
}

/// The client's orders to its pets.
pub fn request<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    function: u32,
    v: &[Value],
    clock: Clock,
) {
    let Some(owner) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let long = |i: usize| -> Option<u64> {
        v.iter()
            .filter_map(|x| match x {
                Value::Long(l) => Some(*l),
                _ => None,
            })
            .nth(i)
    };
    // An attack order from a rider is refused, with its own text.
    if function == function::ATTACK && crate::mounts::refuses_pet_order(s, net, world) {
        return;
    }
    if matches!(function, function::ATTACK | function::RETURN) {
        if let Some(wait) = owe(s, owner, clock.local.0) {
            let t = progress::text_with(
                text::TOO_MANY,
                vec![auberean_script::native_object::Variable {
                    id: text::WAIT_VAR,
                    kind: 4,
                    value: auberean_script::native_object::VariableValue::Double(
                        f64::from(wait).to_bits(),
                        text::WAIT_DIGITS,
                    ),
                }],
            );
            progress::private(net, world, &crate::chat::show_text(1, t));
            return;
        }
    }
    match function {
        function::MODE => {
            let (Some(Value::Int(mode)), Some(pet)) = (v.first(), long(0)) else {
                s.unhandled.note("a pet's mode that does not read");
                return;
            };
            let mode = *mode;
            let Some(p) = s.pets.by_id.get_mut(&pet).filter(|p| p.owner == owner) else {
                return;
            };
            p.mode = mode;
            p.flags = p.flags & !flag::BEHAVIOR | mode_flags(mode);
            progress::private(net, world, &mode_update(pet, mode));
        }
        function::NAME => {
            let typed = v.iter().find_map(|x| match x.root() {
                Some(crate::calls::Node::Text(t)) => Some(t.clone()),
                _ => None,
            });
            let (Some(typed), Some(pet)) = (typed, long(0)) else {
                s.unhandled.note("a pet's name that does not read");
                return;
            };
            rename(s, net, world, owner, pet, &typed);
        }
        function::DIE => {
            let pet = long(0).unwrap_or(0);
            let pets: Vec<u64> = s
                .pets
                .of(owner)
                .into_iter()
                .filter(|p| pet == 0 || *p == pet)
                .collect();
            for p in pets {
                dies(s, net, p, 0, clock);
            }
        }
        function::ATTACK => {
            let Some(target) = long(0) else {
                s.unhandled.note("a pet's attack that does not read");
                return;
            };
            order_attack(s, net, world, owner, target);
        }
        function::RETURN => {
            for id in s.pets.of(owner) {
                if let Some(p) = s.pets.by_id.get_mut(&id) {
                    p.order = Order::Return;
                    p.next_plan = clock.local.0;
                    p.assist = None;
                }
                calm(s, net, id);
            }
        }
        _ => {}
    }
}

/// The debt of orders above which one is refused, and what each order adds to it, in seconds
/// (2017: 1,732 of 1,732 orders kept to it; each refusal's wait the debt after it less five, 6 of
/// 6 within 0.09 s).
pub const ORDERS_OWED: f64 = 5.0;
pub const ORDER_COST: f64 = 1.0;

/// An order runs up its owner's debt (refused or not); refused (the seconds to wait) when the
/// debt before it is above the limit. The debt drains a second a second.
fn owe<S: Store>(s: &mut Server<S>, owner: u64, now: f64) -> Option<f32> {
    let (debt, at) = s.pets.owed.get(&owner).copied().unwrap_or((0.0, now));
    let before = (debt - (now - at)).max(0.0);
    let after = before + ORDER_COST;
    s.pets.owed.insert(owner, (after, now));
    #[allow(clippy::cast_possible_truncation)]
    (before > ORDERS_OWED).then_some((after - ORDERS_OWED) as f32)
}

/// The longest name a pet may take, in UTF-16 units. The servers refused every recorded name of 27
/// or more (11 of 11, up to 47) and took every one of 13 or less; none between was asked for.
pub const MOST_NAME_UNITS: usize = 26;

/// What its owner is told when a pet can't take this name, written as the server keeps it: one too
/// long, or one with anything but letters of any alphabet, spaces, apostrophes and hyphens (digits
/// were refused, 3 names of 3; an accented letter and a space were taken).
#[must_use]
pub fn name_refused(name: &str) -> Option<(u32, u32)> {
    if name.encode_utf16().count() > MOST_NAME_UNITS {
        return Some(text::NAME_TOO_LONG);
    }
    let allowed = |c: char| c.is_alphabetic() || matches!(c, ' ' | '\'' | '-');
    (name.is_empty() || !name.chars().all(allowed)).then_some(text::NAME_NOT_ALLOWED)
}

/// A pet renamed: kept in the written form, its owner told "Name (kind)" as its name and everyone
/// who sees it shown the same; nothing new shown when it is the name it has. A name it can't take
/// changes nothing, and its owner is told why.
fn rename<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    owner: u64,
    pet: u64,
    typed: &str,
) {
    let name = written(typed);
    let Some(p) = s.pets.by_id.get_mut(&pet).filter(|p| p.owner == owner) else {
        return;
    };
    if let Some(why) = name_refused(&name) {
        let told = crate::chat::show_text(1, progress::text(why));
        progress::private(net, world, &told);
        return;
    }
    let template = p.template;
    let same = p.name.as_deref() == Some(name.as_str());
    p.name = Some(name.clone());
    let kind = s
        .catalog
        .get(template)
        .and_then(|t| t.description.name)
        .unwrap_or_default();
    let shown = shown_name(&name, kind);
    progress::private(net, world, &name_update(pet, shown.clone()));
    if same {
        return;
    }
    let as_info = string_info(&shown);
    if let Some(t) = s.things.get_mut(pet) {
        t.named = Some(as_info.clone());
    }
    let m = ServerMessage::UpdateStringInfoVisual(VisualQuality {
        object: crate::inventory::visual(s, pet),
        quality: NAME_QUALITY,
        value: as_info,
    });
    crate::combat::to_all_seeing(s, net, pet, &m);
}

/// How far from its owner an attack order's target may be, in meters (2017: 650 of 662 accepted
/// within 32.1 m, 146 of 150 refused beyond it, owner to target).
pub const ATTACK_RANGE: f32 = 32.0;

/// An attack order: every pet of the owner after the target, unless the target is too far from
/// the owner.
fn order_attack<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    owner: u64,
    target: u64,
) {
    let Some(at) = s
        .things
        .get(target)
        .filter(|t| t.alive())
        .and_then(|t| t.at)
    else {
        return;
    };
    let Some(from) = s.players.get(&world).map(|p| p.position) else {
        return;
    };
    if let Err(err) = crate::combat::may_attack_thing(s, world, target) {
        progress::private(net, world, &crate::combat::failed(err));
        return;
    }
    if crate::presence::distance(&from, &at).is_none_or(|d| d > ATTACK_RANGE) {
        progress::private(
            net,
            world,
            &crate::chat::show_text(1, progress::text(text::TOO_FAR)),
        );
        return;
    }
    for id in s.pets.of(owner) {
        if let Some(p) = s.pets.by_id.get_mut(&id) {
            p.order = Order::Follow;
        }
        take_on(s, net, id, target);
    }
}

/// A pet takes on a target: its combat mode (when it was at peace), its target.
fn take_on<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, target: u64) {
    if never_attacks(s, id) {
        return;
    }
    let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
        return;
    };
    if c.mode == Mode::Dead || c.target == Some(target) {
        return;
    }
    let fresh = c.mode != Mode::Chasing;
    c.mode = Mode::Chasing;
    c.target = Some(target);
    if fresh {
        crate::combat::creature_engages(s, net, id, target);
    }
    let open = s
        .pets
        .by_id
        .get_mut(&id)
        .and_then(|p| p.goal.take())
        .is_some();
    // A move to its owner still under way ends with the fight: its kind's own speed, right after
    // its fighting mode (2017: 324 of 380 such entries, the kind's own 332 of 332, after the mode
    // 355 of 405).
    if fresh && open {
        let own = kind_velocity(s, id);
        tell_speed(s, net, id, VELOCITY_FACTOR * own);
    }
}

/// A pet out of a fight: at peace.
fn calm<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
        return;
    };
    c.target = None;
    if c.mode != Mode::Chasing {
        return;
    }
    c.mode = Mode::Idle;
    crate::combat::creature_calms(s, net, id);
}

/// How long after it dies a pet's corpse comes, in seconds (2017: a median 1.5 s after it is taken
/// off the list).
pub const CORPSE_AFTER: f64 = 1.5;

/// A pet dies (killed by `killer`, ordered, or with its owner: 0): taken off its owner's list,
/// then it falls as a creature does, in one tick; its corpse comes after.
pub fn dies<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, killer: u64, clock: Clock) {
    let Some(pet) = s.pets.by_id.remove(&id) else {
        return;
    };
    if let Some(world) = conn_of(s, pet.owner) {
        progress::private(net, world, &removed(id));
    }
    if let Some(t) = s.things.get_mut(id) {
        t.health = 0;
        if let Some(c) = t.creature.as_mut() {
            c.mode = Mode::Dead;
            c.target = None;
        }
    }
    let pose = crate::death::falls(s, net, id, clock);
    match s.pets.kind_of(pet.effect).and_then(|k| k.corpse_after) {
        Some(after) => s.after(
            clock,
            after,
            crate::Timer::Corpse {
                creature: id,
                killer,
                pose,
            },
        ),
        // No corpse: the body leaves as long after as a corpse's body would.
        None => s.after(clock, NO_CORPSE_LEAVES_AFTER, crate::Timer::Vanish { id }),
    }
}

/// How long after its death a pet that leaves no corpse is gone, in seconds (2017: the rift
/// 3.23-3.31 s, 21 of 21 killed).
pub const NO_CORPSE_LEAVES_AFTER: f64 = 3.27;

/// An owner dies: its pets die with it, in its death tick.
pub fn owner_dies<S: Store>(s: &mut Server<S>, net: &mut ServerNet, owner: u64, clock: Clock) {
    for id in s.pets.of(owner) {
        dies(s, net, id, 0, clock);
    }
}

/// An owner leaves the world: its pets leave it too (nothing more is sent to the owner).
pub fn owner_leaves<S: Store>(s: &mut Server<S>, net: &mut ServerNet, owner: u64) {
    for id in s.pets.of(owner) {
        s.pets.by_id.remove(&id);
        crate::death::vanish(s, net, id);
    }
    s.pets.owed.remove(&owner);
}

/// An owner has come out of portal space somewhere else: its pets are put beside it there.
pub fn owner_arrives<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, time: f64) {
    let Some((owner, at)) = s.players.get(&world).map(|p| (p.character, p.position)) else {
        return;
    };
    for id in s.pets.of(owner) {
        let effect = s.pets.by_id.get(&id).map_or(0, |p| p.effect);
        if s.pets.kind_of(effect).is_some_and(|k| k.stays) {
            continue;
        }
        calm(s, net, id);
        relocate(s, net, id, beside(&at), time);
    }
}

/// Put a pet somewhere else at once: the clients that held it and do not see the new place see it
/// leave; those that see the new place are told of it there.
fn relocate<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    to: StoredPosition,
    time: f64,
) {
    if let Some(p) = s.pets.by_id.get_mut(&id) {
        p.goal = None;
    }
    let Some(t) = s.things.get_mut(id) else {
        return;
    };
    let (instance, stamp) = (t.instance, t.position_stamp.wrapping_add(1));
    let leave = ServerMessage::ObjectLeaves(auberean_protocol::physics::ObjectEvent {
        sender: ObjectStamp {
            id,
            instance_stamp: instance,
            other_stamp: 0,
        },
        action: auberean_protocol::physics::Leave { stamp },
    })
    .encode();
    for (conn, p) in &mut s.players {
        if !crate::presence::in_view(&p.position, to.cell, to.origin) {
            if let Some(seen) = p.known.remove(&id) {
                net.send_ephemeral(
                    *conn,
                    auberean_protocol::queue::EVENT,
                    seen.position_stream,
                    leave.clone(),
                );
            }
        }
    }
    let Some(t) = s.things.get_mut(id) else {
        return;
    };
    t.at = Some(to);
    t.position_stamp = t.position_stamp.wrapping_add(1);
    if let Some(c) = t.creature.as_mut() {
        c.home = to;
    }
    let m = ObjectPosition {
        sender: ObjectStamp {
            id,
            instance_stamp: t.instance,
            other_stamp: 0,
        },
        time,
        cell: to.cell,
        origin: auberean_primitives::Vec3::new(to.origin[0], to.origin[1], to.origin[2]),
        heading_motion: t.heading_motion,
        flags: 0x4,
        position_stamp: t.position_stamp,
        ..ObjectPosition::default()
    };
    crate::presence::thing_moves(s, net, id, &m, true);
}

/// How often a pet not fighting plans its way to its owner, in seconds (2017: its moves 3.0 s
/// apart, then 6 and 9); how short of its owner it stops (2017: a Lasher a median 3.7 m from a
/// still owner, the others 2.7-3.4 m); how far away it is put beside its owner (2017: 87 jumps from
/// a median 105 m, 74-120 m as the 3 s plan saw it).
pub const PLAN_EVERY: f64 = 3.0;
pub const STOPS_SHORT: f32 = 3.0;
pub const TOO_FAR: f32 = 100.0;
/// How near its owner a pet called back is with it again, in meters.
pub const BACK_WITH: f32 = 5.0;

/// How far from its owner a pet looks for what its mode has it take on, in meters.
pub const ASSIST_RANGE: f32 = 40.0;

/// Every pet acts.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let ids: Vec<u64> = s.pets.by_id.keys().copied().collect();
    for id in ids {
        if let Some(pet) = s.pets.by_id.get(&id).cloned() {
            act(s, net, &pet, clock);
        }
    }
}

fn act<S: Store>(s: &mut Server<S>, net: &mut ServerNet, pet: &Pet, clock: Clock) {
    let id = pet.id;
    let now = clock.local.0;
    let Some((world, owner_at)) = s
        .players
        .iter()
        .find(|(_, p)| p.character == pet.owner)
        .map(|(c, p)| (*c, p.position))
    else {
        return;
    };
    let Some(at) = s.things.get(id).filter(|t| t.alive()).and_then(|t| t.at) else {
        return;
    };
    tend(s, net, pet, clock);
    let away = crate::presence::distance(&at, &owner_at);
    let mut order = pet.order;
    if order == Order::Return && away.is_some_and(|d| d <= BACK_WITH) {
        order = Order::Follow;
        if let Some(p) = s.pets.by_id.get_mut(&id) {
            p.order = Order::Follow;
        }
    }
    let fighting = if order == Order::Return || never_attacks(s, id) {
        None
    } else {
        let current = s
            .things
            .get(id)
            .and_then(|t| t.creature.as_ref())
            .and_then(|c| c.target)
            .filter(|t| s.things.get(*t).is_some_and(Thing::alive))
            .filter(|t| crate::combat::may_attack_thing(s, world, *t).is_ok());
        current.or_else(|| unbidden(s, pet, &at))
    };
    if let Some(t) = fighting {
        take_on(s, net, id, t);
        fight(s, net, id, t, clock);
        return;
    }
    calm(s, net, id);
    if s.pets.kind_of(pet.effect).is_some_and(|k| k.stays) {
        return;
    }
    if now >= pet.next_plan {
        if let Some(p) = s.pets.by_id.get_mut(&id) {
            p.next_plan = now + PLAN_EVERY;
        }
        if away.is_none_or(|d| d > TOO_FAR) && pet.flags & flag::TELEPORT_TO_MASTER != 0 {
            relocate(s, net, id, beside(&owner_at), net.game_time(clock.local));
            return;
        }
        if away.is_some_and(|d| d > STOPS_SHORT) {
            plan(s, net, world, id, &at, &owner_at);
        }
    }
    let goal = s.pets.by_id.get(&id).and_then(|p| p.goal);
    if let Some(goal) = goal {
        if crate::presence::distance(&at, &goal).is_none_or(|d| d <= crate::ai::ARRIVED) {
            if let Some(p) = s.pets.by_id.get_mut(&id) {
                p.goal = None;
            }
            // Arrived: its kind's own speed, told even when unchanged.
            let own = kind_velocity(s, id);
            tell_speed(s, net, id, VELOCITY_FACTOR * own);
        } else {
            let speed = run_speed(s, id);
            crate::ai::step_toward(s, net, id, &goal, speed, 0.0, crate::ai::RUNNING, clock);
        }
    }
}

/// A pet plans its way to its owner: a point short of where its owner stands now. Setting off from
/// standing, its speed is told to those who see it (the faster of its kind's and its owner's);
/// a plan made while it runs tells nothing.
fn plan<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    id: u64,
    at: &StoredPosition,
    owner_at: &StoredPosition,
) {
    let (dx, dy, _) = crate::ai::offset(at, owner_at);
    let flat = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
    let back = STOPS_SHORT.min(flat) / flat;
    let goal = StoredPosition {
        cell: owner_at.cell,
        origin: [
            owner_at.origin[0] - dx * back,
            owner_at.origin[1] - dy * back,
            owner_at.origin[2],
        ],
        rotation: owner_at.rotation,
    };
    if crate::presence::distance(at, &goal).is_some_and(|d| d <= crate::ai::ARRIVED) {
        // Already where it would go: it stays standing.
        return;
    }
    let running = s.pets.by_id.get(&id).is_some_and(|p| p.goal.is_some());
    if let Some(p) = s.pets.by_id.get_mut(&id) {
        p.goal = Some(goal);
    }
    if running {
        return;
    }
    let own = kind_velocity(s, id);
    let v = VELOCITY_FACTOR * own.max(crate::effects::player_speed(s, world).0);
    tell_speed(s, net, id, v);
}

/// A pet's kind's own velocity scale.
fn kind_velocity<S: Store>(s: &Server<S>, id: u64) -> f32 {
    s.things
        .get(id)
        .and_then(|t| s.catalog.get(t.template))
        .and_then(|t| t.props.float(KIND_VELOCITY))
        .unwrap_or(1.0)
}

/// Tell those who see a pet its velocity scale, then its acceleration scale.
fn tell_speed<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, v: f32) {
    if let Some(t) = s.things.get_mut(id) {
        t.velocity_scale = Some(v);
    }
    let sender = crate::inventory::visual(s, id);
    let velocity = ServerMessage::VelocityScale(auberean_protocol::physics::ObjectEvent {
        sender,
        action: ScaleEvent(v),
    });
    crate::combat::to_all_seeing(s, net, id, &velocity);
    let sender = crate::inventory::visual(s, id);
    let acceleration = ServerMessage::AccelerationScale(auberean_protocol::physics::ObjectEvent {
        sender,
        action: ScaleEvent(v * ACCELERATION_FACTOR),
    });
    crate::combat::to_all_seeing(s, net, id, &acceleration);
}

/// A pet's running speed: a creature's of its scale, times its velocity scale over a creature's
/// own (1.75).
fn run_speed<S: Store>(s: &Server<S>, id: u64) -> f32 {
    let scale = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map_or(1.0, |c| c.stats.scale);
    crate::ai::RUN_SPEED * scale * VELOCITY_FACTOR
}

/// How far from its owner a supporting pet may be and still join its owner's fight, in meters
/// (2017: 6 of 24 joined beyond it, 814 of 932 within).
pub const JOINS_WITHIN: f32 = 40.0;
/// How far an aggressive pet looks for a creature to take on, in meters (every class pet's
/// perception; 2017: the nearest creature within it in 59 of 79 unprovoked fights).
pub const AGGRESSIVE_REACH: f32 = 40.0;

/// What a pet takes on by itself, by its mode: supporting, what its owner last hurt, unless it is
/// more than 40 m from its owner; protective, a creature going after its owner; aggressive, the
/// nearest creature within its reach (40 m; a pet that stays, its missile's).
fn unbidden<S: Store>(s: &Server<S>, pet: &Pet, at: &StoredPosition) -> Option<u64> {
    let p = s.players.values().find(|p| p.character == pet.owner)?;
    let alive_creature = |t: u64| {
        s.things
            .get(t)
            .is_some_and(|x| x.alive() && x.creature.is_some() && !s.pets.is_pet(t))
    };
    let fit = |t: u64| {
        alive_creature(t)
            && s.things
                .get(t)
                .and_then(|x| x.at)
                .and_then(|a| crate::presence::distance(&a, at))
                .is_some_and(|d| d <= ASSIST_RANGE)
    };
    if pet.flags & flag::SUPPORT_MASTER != 0 {
        let near_owner =
            crate::presence::distance(&p.position, at).is_some_and(|d| d <= JOINS_WITHIN);
        if let Some(t) = pet.assist.filter(|t| near_owner && fit(*t)) {
            return Some(t);
        }
    }
    if pet.flags & flag::DEFEND_MASTER != 0 {
        let defending = s
            .things
            .by_id
            .values()
            .filter(|t| fit(t.id))
            .filter(|t| {
                t.creature
                    .as_ref()
                    .is_some_and(|c| c.mode == Mode::Chasing && c.target == Some(pet.owner))
            })
            .filter_map(|t| crate::presence::distance(&t.at?, at).map(|d| (d, t.id)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id);
        if defending.is_some() {
            return defending;
        }
    }
    if pet.flags & flag::AGGRESSIVE != 0 {
        let reach = if s.pets.kind_of(pet.effect).is_some_and(|k| k.stays) {
            reach_of(s, pet.id)
        } else {
            AGGRESSIVE_REACH
        };
        return s
            .things
            .by_id
            .values()
            .filter(|t| t.id != pet.id && alive_creature(t.id))
            .filter_map(|t| crate::presence::distance(&t.at?, at).map(|d| (d, t.id)))
            .filter(|(d, _)| *d <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id);
    }
    None
}

/// How far a pet's attacks reach: its missile's range, else a melee reach.
fn reach_of<S: Store>(s: &Server<S>, id: u64) -> f32 {
    attacks(s, id)
        .iter()
        .filter(|a| a.missile)
        .map(|a| a.max_range)
        .fold(crate::ai::MELEE_REACH, f32::max)
}

/// The owner of a pet hurt a creature: its supporting pets take it on (2017: they joined at the
/// owner's first hit that took health, in that same tick, 422 of 677 timed).
pub fn owner_hurt<S: Store>(s: &mut Server<S>, owner: u64, creature: u64) {
    if s.pets.is_pet(creature) {
        return;
    }
    for p in s.pets.by_id.values_mut() {
        if p.owner == owner && p.order == Order::Follow {
            p.assist = Some(creature);
        }
    }
}

/// How often a pet attacks, in seconds (2017: the Lasher a median 1.18 s on one target).
pub const ATTACK_SECONDS: f64 = 1.18;
/// The share of a pet's attacks that are its missile's when it has one in reach (2017: the
/// Lasher's Hunter's Intuition 70%).
pub const MISSILE_SHARE: f32 = 0.7;

/// A pet fights its target: within its attacks' reach it attacks, else it closes in (a pet that
/// stays only turns to it, and waits).
fn fight<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, target: u64, clock: Clock) {
    let Some(there) = s.things.get(target).and_then(|t| t.at) else {
        return;
    };
    let Some(at) = s.things.get(id).and_then(|t| t.at) else {
        return;
    };
    let Some(kind) = s
        .pets
        .by_id
        .get(&id)
        .and_then(|p| s.pets.kind_of(p.effect))
        .cloned()
    else {
        return;
    };
    let d = crate::presence::distance(&at, &there).unwrap_or(f32::MAX);
    let attacks = attacks(s, id);
    let missile = attacks.iter().find(|a| a.missile).copied();
    let melee = attacks.iter().find(|a| !a.missile).copied();
    let reach = missile.map_or(crate::ai::MELEE_REACH, |m| {
        m.max_range.max(crate::ai::MELEE_REACH)
    });
    if d > reach {
        if kind.stays {
            return;
        }
        let speed = run_speed(s, id);
        crate::ai::step_toward(
            s,
            net,
            id,
            &there,
            speed,
            reach * 0.8,
            crate::ai::RUNNING,
            clock,
        );
        return;
    }
    if !kind.stays {
        crate::ai::face(s, net, id, &there, clock);
    }
    let due = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .is_some_and(|c| clock.local.0 >= c.next_attack);
    if !due {
        return;
    }
    let use_missile = match (missile, melee) {
        (Some(_), Some(_)) => d > crate::ai::MELEE_REACH || s.rng.chance(MISSILE_SHARE),
        (Some(_), None) => true,
        _ => false,
    };
    // A pet whose template names no skills at all strikes as a creature does.
    let skill =
        if use_missile { missile } else { melee }.unwrap_or(crate::missiles::CREATURE_ATTACKS[0]);
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.next_attack = clock.local.0 + ATTACK_SECONDS;
    }
    if skill.missile {
        launch(s, net, id, target, skill, d, clock);
    } else {
        strike(s, net, id, target, skill, &kind, clock);
        if kind.taunts {
            taunted(s, net, target, id, clock);
        }
    }
}

/// How a pet's strike on a creature comes out, and what it takes: a share of strikes land (none
/// critical); a hit takes its kind's number times the pet's level squared, varied either side,
/// less the creature's armor, at least 1 (2017: Lasher hits 200 x U(0.5, 1.5) at levels 52-54,
/// armor a point a point; no criticals in about 30,800 pet strikes). A taunt takes nothing.
fn roll<S: Store>(s: &mut Server<S>, id: u64, target: u64, kind: &PetKind) -> (Outcome, i32) {
    if kind.taunts {
        return (Outcome::Miss, 0);
    }
    if !s.rng.chance(kind.lands) {
        return (Outcome::Evade, 0);
    }
    let level = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map_or(1, |c| c.stats.level);
    let armor = s
        .things
        .get(target)
        .and_then(|t| t.creature.as_ref())
        .map_or(0, |c| c.stats.armor);
    (Outcome::Hit, pet_hit(&mut s.rng, kind, level, armor))
}

/// What a pet's strike comes to once the creature's effects have watched it.
fn answered<S: Store>(
    s: &mut Server<S>,
    target: u64,
    outcome: Outcome,
    damage: i32,
) -> crate::effects::Harm {
    if outcome.landed() {
        crate::effects::answer_harm(s, crate::effects::On::Creature(target), damage, false)
    } else {
        crate::effects::Harm::default()
    }
}

/// A pet's hit at its level on a creature of this armor.
#[must_use]
pub fn pet_hit(rng: &mut crate::rng::Rng, kind: &PetKind, level: i32, armor: i32) -> i32 {
    #[allow(clippy::cast_precision_loss)]
    let l = level.max(1) as f32;
    let base = kind.per_level_squared * l * l;
    let v = rng.between(1.0 - kind.variance, 1.0 + kind.variance);
    #[allow(clippy::cast_precision_loss)]
    let hit = base * v - armor as f32;
    dereth_primitives::num::to_i32(hit.round()).max(1)
}

/// Is a pet's strike told as a basic attack? By the skill, each always one way or the other, and
/// nothing in its definition tells which (2017, pets on creatures, with the flag: 490 13,839, 491
/// 13,413, 1098 376, 1023 288, 1845 18, 1846 15, 492 7, 1818 3; without it: 848 14,149, 865 13,271,
/// 838 4,335, 847 415, 1024 26).
#[must_use]
pub const fn strikes_as_basic(skill: u32) -> bool {
    matches!(skill, 490 | 491 | 492 | 1023 | 1098 | 1818 | 1845 | 1846)
}

/// A pet strikes a creature in reach: one story of the pet's with a result for each of its hits
/// (with the basic-attack flag by its skill), the hits landing together as its blow reaches the
/// creature.
fn strike<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    target: u64,
    skill: AttackSkill,
    kind: &PetKind,
    clock: Clock,
) {
    let clock_free = crate::combat::CREATURE_TIME_SCALE * s.rng.between(0.9, 1.1);
    let mut rolls = Vec::new();
    for _ in 0..kind.hits.max(1) {
        let (outcome, damage) = roll(s, id, target, kind);
        rolls.push((outcome, answered(s, target, outcome, damage)));
    }
    let swing = |(outcome, harm): (Outcome, crate::effects::Harm)| crate::combat::Swing {
        skill: skill.skill,
        behavior: skill.behavior,
        time_scale: clock_free,
        target,
        outcome,
        damage: harm.left,
        back: harm.back,
    };
    let mut st = crate::combat::story(s, crate::combat::Attacker::Thing(id), swing(rolls[0]));
    if let ServerMessage::DoStory(e) = &mut st {
        e.action.bits = if strikes_as_basic(skill.skill) {
            crate::combat::BASIC_SWING
        } else {
            crate::missiles::SPECIAL
        };
        for r in rolls.iter().skip(1) {
            e.action.hooks.push(swing(*r).hook());
        }
    }
    crate::combat::to_all_seeing(s, net, id, &st);
    let lands = crate::combat::pet_strike_seconds(skill.skill) / f64::from(clock_free);
    for (outcome, harm) in rolls {
        if outcome.landed() {
            s.after(
                clock,
                lands,
                crate::Timer::ThingStrikeLands {
                    by: id,
                    target,
                    skill: skill.skill,
                    harm,
                },
            );
        }
    }
}

/// A pet's missile flies at this many meters a second, after a windup of this many seconds at the
/// ordinary time scale (2017: the Lasher's impact 1.018 s / time scale + distance / 59 m/s after its
/// throw, 2,110 throws).
pub const MISSILE_SPEED: f32 = 59.0;
pub const MISSILE_WINDUP: f32 = 1.018;

/// A pet throws its missile: the throw's story now (the pet's, naming its target), the landing
/// told by what it lands on after the flight.
fn launch<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    target: u64,
    skill: AttackSkill,
    distance: f32,
    clock: Clock,
) {
    let ts = crate::combat::CREATURE_TIME_SCALE * s.rng.between(0.9, 1.1);
    let mut st = crate::combat::story(
        s,
        crate::combat::Attacker::Thing(id),
        crate::combat::Swing {
            skill: skill.skill,
            behavior: skill.behavior,
            time_scale: ts,
            target,
            outcome: Outcome::Hit,
            damage: 0,
            back: 0,
        },
    );
    if let ServerMessage::DoStory(e) = &mut st {
        e.action.bits = crate::missiles::SPECIAL;
        let mut missile = auberean_protocol::types::StoryHook {
            bits: crate::missiles::MISSILE_HOOK,
            target,
            ..auberean_protocol::types::StoryHook::default()
        };
        missile.numbers[1] = 1;
        e.action.hooks.truncate(1);
        e.action.hooks.push(missile);
    }
    crate::combat::to_all_seeing(s, net, id, &st);
    let flight = MISSILE_WINDUP / ts + distance / MISSILE_SPEED;
    s.after(
        clock,
        f64::from(flight),
        crate::Timer::PetMissile {
            pet: id,
            target,
            skill,
        },
    );
}

/// A pet's missile lands: what it lands on tells the landing, a result for its target and, for a
/// missile that falls on all near (the rift's), for every creature that near.
pub fn lands<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    pet: u64,
    target: u64,
    skill: AttackSkill,
    clock: Clock,
) {
    let Some(kind) = s
        .pets
        .by_id
        .get(&pet)
        .and_then(|p| s.pets.kind_of(p.effect))
        .cloned()
    else {
        return;
    };
    let Some(there) = s
        .things
        .get(target)
        .filter(|t| t.alive())
        .and_then(|t| t.at)
    else {
        return;
    };
    let mut hit = vec![target];
    if kind.area > 0.0 {
        hit.extend(
            s.things
                .by_id
                .values()
                .filter(|t| t.id != target && t.alive() && t.creature.is_some())
                .filter(|t| !s.pets.is_pet(t.id) && s.players.values().all(|p| p.character != t.id))
                .filter(|t| {
                    t.at.and_then(|a| crate::presence::distance(&a, &there))
                        .is_some_and(|d| d <= kind.area)
                })
                .map(|t| t.id),
        );
    }
    let mut hooks = Vec::new();
    let mut landed = Vec::new();
    for t in hit {
        let (outcome, damage) = roll(s, pet, t, &kind);
        let harm = answered(s, t, outcome, damage);
        hooks.push(
            crate::combat::Swing {
                skill: skill.skill,
                behavior: skill.behavior,
                time_scale: 1.0,
                target: t,
                outcome,
                damage: harm.left,
                back: harm.back,
            }
            .hook(),
        );
        if outcome.landed() {
            landed.push((t, harm));
        }
    }
    let told = ServerMessage::DoStory(auberean_protocol::physics::ObjectEvent {
        sender: crate::inventory::visual(s, target),
        action: auberean_protocol::types::PhysicsStory {
            bits: crate::missiles::SPECIAL_ARRIVAL,
            attacker: pet,
            skill: skill.skill,
            hooks,
            ..auberean_protocol::types::PhysicsStory::default()
        },
    });
    crate::combat::to_all_seeing(s, net, target, &told);
    for (t, harm) in landed {
        struck_by_pet(s, net, t, (pet, skill.skill), harm.left, clock);
        crate::effects::harm_back(s, net, pet, t, harm.back, clock);
    }
}

/// A creature a pet taunted may turn on it, as on a pet that strikes it.
fn taunted<S: Store>(s: &mut Server<S>, net: &mut ServerNet, target: u64, pet: u64, clock: Clock) {
    turn_on_pet(s, net, target, pet, clock);
}

/// Does this pet never attack: its template names skills, and none of them attacks, whatever is on
/// it?
fn never_attacks<S: Store>(s: &Server<S>, id: u64) -> bool {
    let Some(stats) = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| &c.stats)
    else {
        return false;
    };
    !stats.skills.is_empty()
        && stats.skills.iter().all(|(skill, rating)| {
            crate::missiles::attack_skill(&s.skill_table, *skill, *rating).is_none()
        })
}

/// A pet's attacks: its template's skills that attack, as the effects on it allow (what its owner
/// put on it may open one and bar another): one that asks for an effect first.
fn attacks<S: Store>(s: &Server<S>, id: u64) -> Vec<AttackSkill> {
    let Some(stats) = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| &c.stats)
    else {
        return Vec::new();
    };
    let on = crate::effects::records_of(s, crate::effects::On::Creature(id));
    let carries = |state: &u32| on.iter().any(|e| e.state == *state);
    let usage = |skill: u32| {
        s.skill_table
            .skills
            .get(&skill)
            .and_then(|d| d.usage.as_ref())
    };
    let mut open: Vec<AttackSkill> = stats
        .skills
        .iter()
        .filter(|(skill, _)| {
            usage(*skill).is_none_or(|u| {
                u.required_effects.iter().all(carries) && !u.barring_effects.iter().any(carries)
            })
        })
        .filter_map(|(skill, rating)| {
            crate::missiles::attack_skill(&s.skill_table, *skill, *rating)
        })
        .collect();
    open.sort_by_key(|a| usage(a.skill).is_none_or(|u| u.required_effects.is_empty()));
    open
}

/// The chance a creature fighting another turns on a pet that strikes it (2017: 590 of 958).
pub const TURNS_ON_PET: f32 = 0.62;

/// A strike lands on a thing: a pet loses health (shown wounded at 80% or less, dying with none
/// left), and one that defends itself fights back what struck it when it fights nothing else; a
/// creature a pet struck is hurt as its owner's, and fights the pet (turning from another three
/// times in five).
pub fn struck<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    target: u64,
    (by, skill): (u64, u32),
    damage: i32,
    clock: Clock,
) {
    if let Some(pet) = s.pets.by_id.get(&target).cloned() {
        let Some(t) = s.things.get_mut(target) else {
            return;
        };
        #[allow(clippy::cast_precision_loss)]
        let was = t.health as f32 / t.max_health.max(1) as f32;
        t.health = (t.health - damage).max(0);
        t.last_hurt = clock.local.0;
        #[allow(clippy::cast_precision_loss)]
        let share = t.health as f32 / t.max_health.max(1) as f32;
        if t.health == 0 {
            dies(s, net, target, by, clock);
            return;
        }
        crate::combat::wounded(s, net, target, share, was);
        let free = s
            .things
            .get(target)
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.target.is_none());
        if free && pet.order == Order::Follow && pet.flags & flag::SELF_DEFENDING != 0 {
            take_on(s, net, target, by);
        }
        return;
    }
    struck_by_pet(s, net, target, (by, skill), damage, clock);
}

/// A pet's hit lands on a creature: it is hurt as the pet's owner's, and fights the pet (turning
/// from another three times in five).
fn struck_by_pet<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    target: u64,
    (by, skill): (u64, u32),
    damage: i32,
    clock: Clock,
) {
    let Some(owner) = s.pets.by_id.get(&by).map(|p| p.owner) else {
        return;
    };
    crate::combat::hurt_creature_by_skill(s, net, target, (owner, by, skill), damage, clock);
    turn_on_pet(s, net, target, by, clock);
}

/// A creature a pet struck or taunted fights the pet: at once when it fights nothing, turning from
/// another three times in five.
fn turn_on_pet<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    target: u64,
    by: u64,
    clock: Clock,
) {
    let Some(c) = s
        .things
        .get(target)
        .filter(|t| t.alive())
        .and_then(|t| t.creature.as_ref())
    else {
        return;
    };
    match (c.mode, c.target) {
        (Mode::Chasing, Some(t)) if t != by => {
            if s.rng.chance(TURNS_ON_PET) {
                if let Some(c) = s.things.get_mut(target).and_then(|t| t.creature.as_mut()) {
                    c.target = Some(by);
                    c.lost_since = None;
                }
            }
        }
        (Mode::Chasing, _) => {}
        _ => crate::ai::go_after(s, net, target, by, clock),
    }
}
