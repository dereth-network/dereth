//! What the game's rules say a combat story's hooks show, asked of the objects' scripts as the
//! story plays on the attacker's body.
//!
//! - **A hit's effects** ([`Scripts::hit_effects`]): the attacker's scripts are handed a new
//!   object to fill for the story's skill, the attack hook and the attack's result: the effects
//!   on the attacker, the target and the weapon, and the behaviors the target and the weapon
//!   play. The engine reads it back whatever the call answers; an object with scripts always
//!   has effects to show (none set is none shown).
//! - **The damage shown** ([`Scripts::show_damage`]): the target's scripts are told who struck
//!   whom with what skill (none for a basic attack), the result, and the changes to both sides'
//!   health and player-killer damage. They write the combat lines into the chat and send the
//!   floating text over the target (and over the attacker, for damage reflected) to the panels.
//! - **Whether a skill harms** ([`Scripts::is_skill_harmful`]), asked of the attacker's scripts
//!   when the target is the player, to tell the player who last attacked it.
//! - **The missile an attack fires** ([`Scripts::missile_info`]): the scripts of the body playing
//!   the story are handed a missile to fill for the skill, the attack hook, the attacker and the
//!   target: the object it is made from, where on the attacker it waits, how it flies, what it aims
//!   at and what it passes through (see `auberean_glue::missile`). The engine reads back whichever
//!   missile the out argument then names.
//!
//! The physics objects' natives the floating text reads: whether an object stands outside, and
//! how far it is from a position.

use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::{BehaviorParams, Field, NativeObject};
use auberean_script::value::{tag, NativeId, NULL};

use crate::host::{host, CallResult, Scripts};
use crate::skills::{Asker, Slot};

/// The interface functions a story's hooks call on an object's scripts, by id.
pub mod function {
    /// The effects and behaviors a hit shows, into an out argument.
    pub const HIT_EFFECTS: u32 = 0x00B4_00BF;
    /// The damage a hit shows.
    pub const SHOW_DAMAGE: u32 = 0x00B4_00C0;
    /// Whether a skill harms whom it is used on.
    pub const IS_SKILL_HARMFUL: u32 = 0x00B4_00C5;
    /// The missile an attack makes or fires, into an out argument.
    pub const MISSILE_INFO: u32 = 0x00B4_00BE;
}

/// The scripts' notice that carries a hit's floating text to the panels: the object it shows
/// over, and the text.
pub const DO_3D_DAMAGE: u32 = 0x008D << 16 | 0x0001;

/// The object a hit's effects are filled into.
const FX_AND_BEHAVIOR_INFO: u32 = auberean_glue::combat::FX_AND_BEHAVIOR_INFO;

pub use auberean_glue::missile::MissileInfo;
use auberean_glue::missile::{PhysicsValue, MISSILE_INFO};

/// The effects and behaviors a hit shows: effects by number (0 for none) on the attacker, the
/// target and the weapon; behaviors the target and the weapon play (`None` for none set).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HitEffects {
    pub attacker_fx: u32,
    pub target_fx: u32,
    pub weapon_fx: u32,
    pub target_behavior: Option<BehaviorParams>,
    pub weapon_behavior: Option<BehaviorParams>,
}

impl HitEffects {
    /// What the scripts filled into the object they were handed.
    fn of(o: Option<&NativeObject>) -> Self {
        let Some(NativeObject::Adaptor { fields, .. }) = o else {
            return Self::default();
        };
        let word = |k: &str| match fields.get(k) {
            Some(Field::Word(v)) => *v,
            _ => 0,
        };
        let behavior = |k: &str| match fields.get(k) {
            Some(Field::Value(b)) => match &**b {
                NativeObject::Behavior(b) => Some((**b).clone()),
                _ => None,
            },
            _ => None,
        };
        Self {
            attacker_fx: word("AttackerFX"),
            target_fx: word("TargetFX"),
            weapon_fx: word("WeaponFX"),
            target_behavior: behavior("TargetBehavior"),
            weapon_behavior: behavior("WeaponBehavior"),
        }
    }
}

/// A hit as the damage hook shows it: who struck whom with what skill (0 for a basic attack), the
/// attack's result, and the changes to the target's and the attacker's health and player-killer
/// damage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Damage {
    pub attacker: u64,
    pub target: u64,
    pub skill: u32,
    pub result: u32,
    pub target_health: i32,
    pub attacker_health: i32,
    pub target_pk: i32,
    pub attacker_pk: i32,
}

/// The two words of a 64-bit argument, each marked as half of one.
fn long(v: u64) -> [(u32, u8); 2] {
    #[allow(clippy::cast_possible_truncation)]
    let lo = v as u32;
    let hi = (v >> 32) as u32;
    [(lo, tag::LONG | tag::MIXED), (hi, tag::LONG | tag::MIXED)]
}

impl Scripts {
    /// The effects and behaviors a hit of the story's skill shows at an attack hook, by the
    /// attacker's rules: a new object handed to the attacker's scripts in an out argument, with
    /// the attack's result, the hook and the skill (pushed in that order), read back when the call
    /// ends. `None` when the attacker is not here or has no scripts.
    // D2-01118, D2-00620
    pub fn hit_effects(
        &mut self,
        attacker: u64,
        skill: u32,
        attack_hook: u32,
        result: u32,
    ) -> Option<(CallResult, HitEffects)> {
        let mut a = Asker {
            s: self,
            incomplete: Vec::new(),
        };
        let args = [
            (result, tag::MIXED),
            (attack_hook, tag::MIXED),
            (skill, tag::MIXED),
        ];
        let answer = a.ask_object(
            attacker,
            None,
            function::HIT_EFFECTS,
            &[Slot::New(FX_AND_BEHAVIOR_INFO)],
            &args,
            false,
        )?;
        let fx = HitEffects::of(answer.objects.first().and_then(Option::as_ref));
        Some((answer.result, fx))
    }

    /// Shows a hit on the target's scripts: the attacker-player-killer change, the target's, the
    /// attacker's and the target's health changes, the result, the skill, the target and the
    /// attacker, pushed in that order (each id as two words). `None` when the target is not here
    /// or has no scripts.
    // D2-01097, D2-00621
    pub fn show_damage(&mut self, d: &Damage) -> Option<CallResult> {
        let mut a = Asker {
            s: self,
            incomplete: Vec::new(),
        };
        #[allow(clippy::cast_sign_loss)]
        let words = [
            d.attacker_pk as u32,
            d.target_pk as u32,
            d.attacker_health as u32,
            d.target_health as u32,
            d.result,
            d.skill,
        ];
        let mut args: Vec<(u32, u8)> = words.iter().map(|w| (*w, tag::MIXED)).collect();
        args.extend(long(d.target));
        args.extend(long(d.attacker));
        let answer = a.ask_object(d.target, None, function::SHOW_DAMAGE, &[], &args, false)?;
        Some(answer.result)
    }

    /// The missile an attack fires at an attack hook, by the rules of the body playing the story:
    /// a new missile handed to the body's scripts in an out argument, with the target, the
    /// attacker (each as two words), the attack hook and the skill, pushed in that order, and read
    /// back when the call ends (one the scripts did not fill is a new one). `None` when the body is
    /// not here or has no scripts.
    // D2-01129, D2-00619
    pub fn missile_info(
        &mut self,
        body: u64,
        skill: u32,
        attack_hook: u32,
        attacker: u64,
        target: u64,
    ) -> Option<(CallResult, MissileInfo)> {
        let mut a = Asker {
            s: self,
            incomplete: Vec::new(),
        };
        let mut args: Vec<(u32, u8)> = Vec::with_capacity(6);
        args.extend(long(target));
        args.extend(long(attacker));
        args.extend([(attack_hook, tag::MIXED), (skill, tag::MIXED)]);
        let answer = a.ask_object(
            body,
            None,
            function::MISSILE_INFO,
            &[Slot::New(MISSILE_INFO)],
            &args,
            false,
        )?;
        let missile = answer
            .objects
            .first()
            .and_then(Option::as_ref)
            .and_then(PhysicsValue::missile)
            .unwrap_or_default();
        Some((answer.result, missile))
    }

    /// Whether a skill harms whom it is used on, by the attacker's rules: any word but 0 is yes.
    /// `None` when the attacker is not here or has no scripts.
    // D2-01188, D2-00626
    pub fn is_skill_harmful(&mut self, attacker: u64, skill: u32) -> Option<(CallResult, bool)> {
        let mut a = Asker {
            s: self,
            incomplete: Vec::new(),
        };
        let answer = a.ask_object(
            attacker,
            None,
            function::IS_SKILL_HARMFUL,
            &[],
            &[(skill, tag::MIXED)],
            false,
        )?;
        // The whole word is the answer: the scripts answer a flag's bit, not 1.
        Some((answer.result, answer.value & 0xFFFF_FFFF != 0))
    }
}

/// Where an object stands in the world, as its physics object has it: the player's body, any
/// other object's last place while it is in the world. `None` without one.
pub(crate) fn standing(c: &mut Call<'_>, object: u64) -> Result<Option<(u32, [f32; 3])>, Stop> {
    let h = host(c)?;
    if h.model.player == Some(object) {
        if let Some((cell, origin, _)) = h.my_position {
            return Ok(Some((cell, origin)));
        }
    }
    Ok(h.model
        .object(object)
        .filter(|o| o.in_world)
        .and_then(|o| o.place.as_ref())
        .map(|p| (p.cell, [p.origin.x, p.origin.y, p.origin.z])))
}

/// Whether the receiver's object stands outside: in one of a landblock's outdoor cells (the
/// cell's low half below 0x100). One with no place in the world is not.
// D2-04729, D2-08177
fn is_outside(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = crate::host::receiver_object(c)?;
    let outside = standing(c, object)?.is_some_and(|(cell, _)| cell & 0xFFFF < 0x100);
    c.write(-2, u32::from(outside), tag::MIXED)?;
    Ok(Status::Success)
}

/// How far the receiver's object stands from the position the argument names (a float, below
/// it); -1 for one with no place in the world.
// D2-04700, D2-08180, D2-05987
fn calculate_distance(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = crate::host::receiver_object(c)?;
    let to = c.arg(0)?;
    if to == NULL {
        return Err(Stop::NullReference);
    }
    let there = match c.vm.heap.native(to)? {
        NativeObject::Position { cell, frame } => (
            *cell,
            [
                f32::from_bits(frame[0]),
                f32::from_bits(frame[1]),
                f32::from_bits(frame[2]),
            ],
        ),
        _ => return Err(c.fail("not a position")),
    };
    let d = standing(c, object)?.map_or(-1.0, |here| {
        auberean_glue::position::distance_between(here, there)
    });
    c.write(-3, d.to_bits(), tag::MIXED)?;
    Ok(Status::Success)
}

/// The physics object's natives the floating text reads.
pub(crate) fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    const PHYSICS_OBJECT: u32 = 0x80;
    [
        (NativeId::new(PHYSICS_OBJECT, 0x36), is_outside as NativeFn),
        (NativeId::new(PHYSICS_OBJECT, 0x3B), calculate_distance),
    ]
    .into_iter()
}
