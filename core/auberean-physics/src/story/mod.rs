//! Combat stories: what the server tells observers of an attack, played on the attacker's body in
//! step with its animation.
//!
//! A story ([`PhysicsStory`]) names its attacker, the behavior the attacker plays, the attack's
//! context and skill, and its hooks ([`StoryHookData`]), each tied to the animation hook it waits
//! for: a hit (effects and behaviors on the attacker, target and weapon, and the damage shown), the
//! missiles to make and hold ready, or one missile to fire. The game's rules say which effects and
//! behaviors a hit shows ([`FxAndBehaviorInfo`]) and how a missile flies ([`MissileInfo`]);
//! [`StoryPlayer`] plays a story through its body's host.
//!
//! How a behavior is played is the animation's business: the story carries it as any type that
//! packs and says whether it names a behavior ([`Behavior`]).

use auberean_assets::Context;
use auberean_dat::reader::Result;

use crate::collision::CollisionSettings;
use crate::missile::{MissileParameters, MissileTargetingParameters};
use crate::pack::{read_packs, write_packs, Pack, Reader, Writer};
use crate::target::NO_OBJECT;

mod player;
pub use player::{StoryHost, StoryPlayer, FADE_SLIDER, MISSILE_TIMEOUT, STORY_HOOK};

/// How a behavior is played on an object, as a story carries it.
pub trait Behavior: Pack + Clone + Default + PartialEq + std::fmt::Debug {
    /// Whether it names a behavior to play.
    fn is_valid(&self) -> bool;
    /// Tags it with the context its end is reported under.
    fn set_context(&mut self, context: u32);
}

/// What a story hook does.
pub mod kind {
    pub const UNDEF: u32 = 0;
    /// A hit: effects, behaviors and the damage shown.
    pub const DAMAGE: u32 = 1;
    /// Makes the story's missiles and puts them in the attacker's hand.
    pub const CREATE_MISSILES: u32 = 2;
    /// Fires one missile.
    pub const FIRE_MISSILE: u32 = 3;
}

/// Which fields of a hook are sent: the low four bits are its kind.
pub mod hook_bits {
    pub const KIND: u32 = 0xF;
    pub const WEAPON: u32 = 0x20;
    pub const ATTACK_RESULT: u32 = 0x80;
    pub const TARGET: u32 = 0x100;
    pub const ANIM_HOOK: u32 = 0x200;
    pub const ATTACK_HOOK: u32 = 0x400;
    pub const TARGET_HEALTH: u32 = 0x800;
    pub const ATTACKER_HEALTH: u32 = 0x1000;
    pub const TARGET_PK_DAMAGE: u32 = 0x2000;
    pub const ATTACKER_PK_DAMAGE: u32 = 0x4000;
}

/// One step of a story.
// D2-15861
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoryHookData {
    /// See [`kind`].
    pub kind: u32,
    pub target: u64,
    // D2-15849
    pub weapon: u64,
    pub attack_result: u32,
    /// The animation hook it waits for: the attacker's animation counts them from 0.
    pub anim_hook: u32,
    pub attack_hook: u32,
    pub target_health_change: i32,
    pub attacker_health_change: i32,
    pub target_pk_damage: i32,
    // D2-00412
    pub attacker_pk_damage: i32,
}

impl Default for StoryHookData {
    // D2-15836
    fn default() -> Self {
        Self {
            kind: kind::UNDEF,
            target: NO_OBJECT,
            weapon: NO_OBJECT,
            attack_result: 0,
            anim_hook: 0,
            attack_hook: 0,
            target_health_change: 0,
            attacker_health_change: 0,
            target_pk_damage: 0,
            attacker_pk_damage: 0,
        }
    }
}

impl StoryHookData {
    /// Back to a new hook's values.
    // D2-15837
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Its kind, and a bit for each field that is not its default.
    // D2-15838
    #[must_use]
    pub fn pack_bits(&self) -> u32 {
        let mut b = self.kind & hook_bits::KIND;
        let mut mark = |on: bool, bit: u32| {
            if on {
                b |= bit;
            }
        };
        mark(self.target != NO_OBJECT, hook_bits::TARGET);
        mark(self.weapon != NO_OBJECT, hook_bits::WEAPON);
        mark(self.attack_result != 0, hook_bits::ATTACK_RESULT);
        mark(self.anim_hook != 0, hook_bits::ANIM_HOOK);
        mark(self.attack_hook != 0, hook_bits::ATTACK_HOOK);
        mark(self.target_health_change != 0, hook_bits::TARGET_HEALTH);
        mark(self.attacker_health_change != 0, hook_bits::ATTACKER_HEALTH);
        mark(self.target_pk_damage != 0, hook_bits::TARGET_PK_DAMAGE);
        mark(self.attacker_pk_damage != 0, hook_bits::ATTACKER_PK_DAMAGE);
        b
    }

    pub fn set_weapon(&mut self, weapon: u64) {
        self.weapon = weapon;
    }

    pub fn set_target_pk_damage(&mut self, change: i32) {
        self.target_pk_damage = change;
    }

    pub fn set_attack_result(&mut self, result: u32) {
        self.attack_result = result;
    }

    pub fn set_anim_hook(&mut self, hook: u32) {
        self.anim_hook = hook;
    }

    pub fn set_attack_hook(&mut self, hook: u32) {
        self.attack_hook = hook;
    }

    pub fn set_kind_damage(&mut self) {
        self.kind = kind::DAMAGE;
    }

    pub fn set_kind_create_missiles(&mut self) {
        self.kind = kind::CREATE_MISSILES;
    }

    pub fn set_kind_fire_missile(&mut self) {
        self.kind = kind::FIRE_MISSILE;
    }
}

impl Pack for StoryHookData {
    /// Its pack bits, then each field that is not its default: target, weapon, attack result,
    /// animation hook, attack hook, then the four changes.
    // D2-15862
    fn pack(&self, w: &mut Writer) {
        let b = self.pack_bits();
        w.u32(b);
        if b & hook_bits::TARGET != 0 {
            w.u64(self.target);
        }
        if b & hook_bits::WEAPON != 0 {
            w.u64(self.weapon);
        }
        for (bit, v) in [
            (hook_bits::ATTACK_RESULT, self.attack_result),
            (hook_bits::ANIM_HOOK, self.anim_hook),
            (hook_bits::ATTACK_HOOK, self.attack_hook),
        ] {
            if b & bit != 0 {
                w.u32(v);
            }
        }
        for (bit, v) in [
            (hook_bits::TARGET_HEALTH, self.target_health_change),
            (hook_bits::ATTACKER_HEALTH, self.attacker_health_change),
            (hook_bits::TARGET_PK_DAMAGE, self.target_pk_damage),
            (hook_bits::ATTACKER_PK_DAMAGE, self.attacker_pk_damage),
        ] {
            if b & bit != 0 {
                w.i32(v);
            }
        }
    }

    /// Read over a cleared hook.
    fn unpack(r: &mut Reader<'_>, _ctx: &Context) -> Result<Self> {
        let b = r.u32()?;
        let mut h = Self {
            kind: b & hook_bits::KIND,
            ..Self::default()
        };
        if b & hook_bits::TARGET != 0 {
            h.target = r.u64()?;
        }
        if b & hook_bits::WEAPON != 0 {
            h.weapon = r.u64()?;
        }
        if b & hook_bits::ATTACK_RESULT != 0 {
            h.attack_result = r.u32()?;
        }
        if b & hook_bits::ANIM_HOOK != 0 {
            h.anim_hook = r.u32()?;
        }
        if b & hook_bits::ATTACK_HOOK != 0 {
            h.attack_hook = r.u32()?;
        }
        if b & hook_bits::TARGET_HEALTH != 0 {
            h.target_health_change = r.i32()?;
        }
        if b & hook_bits::ATTACKER_HEALTH != 0 {
            h.attacker_health_change = r.i32()?;
        }
        if b & hook_bits::TARGET_PK_DAMAGE != 0 {
            h.target_pk_damage = r.i32()?;
        }
        if b & hook_bits::ATTACKER_PK_DAMAGE != 0 {
            h.attacker_pk_damage = r.i32()?;
        }
        Ok(h)
    }
}

/// Which fields of a story are sent.
pub mod story_bits {
    pub const ATTACKER: u32 = 0x2;
    pub const ATTACKER_BEHAVIOR: u32 = 0x4;
    pub const CLIENT_CONTEXT: u32 = 0x8;
    pub const SKILL: u32 = 0x10;
    pub const BASIC_ATTACK: u32 = 0x20;
}

/// An attack as its observers are told it.
// D2-15873, D2-15874, D2-15878, D2-15879
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PhysicsStory<B> {
    // D2-15860
    pub attacker: u64,
    pub attacker_behavior: B,
    /// The attack's context on the attacking client.
    // D2-15857
    pub client_context: u32,
    // D2-15858
    pub skill: u32,
    // D2-15859
    pub basic_attack: bool,
    /// In the order of the animation hooks they wait for; hooks waiting for the same one in the
    /// order they were added.
    // D2-15856
    pub hooks: Vec<StoryHookData>,
}

impl<B: Behavior> PhysicsStory<B> {
    /// No attacker, no behavior, no hooks.
    // D2-15877
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A bit for each field that is not its default.
    // D2-15852
    #[must_use]
    pub fn pack_bits(&self) -> u32 {
        let mut b = 0;
        if self.attacker != NO_OBJECT {
            b = story_bits::ATTACKER;
        }
        if self.attacker_behavior.is_valid() {
            b |= story_bits::ATTACKER_BEHAVIOR;
        }
        if self.client_context != 0 {
            b |= story_bits::CLIENT_CONTEXT;
        }
        if self.skill != 0 {
            b |= story_bits::SKILL;
        }
        if self.basic_attack {
            b |= story_bits::BASIC_ATTACK;
        }
        b
    }

    // D2-15854
    pub fn set_skill(&mut self, skill: u32) {
        self.skill = skill;
    }

    pub fn set_basic_attack(&mut self, on: bool) {
        self.basic_attack = on;
    }

    /// A story with nothing to play: no hooks and no attacker behavior.
    // D2-15863
    #[must_use]
    pub fn is_null(&self) -> bool {
        self.hooks.is_empty() && !self.attacker_behavior.is_valid()
    }

    /// The first hook's attack result that is not 0, in hook order; 0 when none is.
    #[must_use]
    pub fn overall_attack_result(&self) -> u32 {
        self.hooks
            .iter()
            .map(|h| h.attack_result)
            .find(|r| *r != 0)
            .unwrap_or(0)
    }

    /// Adds a hook after every hook waiting for the same animation hook or an earlier one.
    // D2-15875
    pub fn add_hook(&mut self, hook: StoryHookData) {
        let at = self
            .hooks
            .partition_point(|h| h.anim_hook <= hook.anim_hook);
        self.hooks.insert(at, hook);
    }
}

impl<B: Behavior> Pack for PhysicsStory<B> {
    /// Its pack bits; the attacker, its behavior, the context and the skill when set; then the
    /// hooks, counted. A basic attack is only a bit.
    // D2-15880
    fn pack(&self, w: &mut Writer) {
        let b = self.pack_bits();
        w.u32(b);
        if b & story_bits::ATTACKER != 0 {
            w.u64(self.attacker);
        }
        if b & story_bits::ATTACKER_BEHAVIOR != 0 {
            self.attacker_behavior.pack(w);
        }
        if b & story_bits::CLIENT_CONTEXT != 0 {
            w.u32(self.client_context);
        }
        if b & story_bits::SKILL != 0 {
            w.u32(self.skill);
        }
        write_packs(w, &self.hooks);
    }

    /// Read over an empty story; the hooks are added one by one, so they come out in order.
    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let b = r.u32()?;
        let mut s = Self::default();
        if b & story_bits::ATTACKER != 0 {
            s.attacker = r.u64()?;
        }
        if b & story_bits::ATTACKER_BEHAVIOR != 0 {
            s.attacker_behavior = B::unpack(r, ctx)?;
        }
        if b & story_bits::CLIENT_CONTEXT != 0 {
            s.client_context = r.u32()?;
        }
        if b & story_bits::SKILL != 0 {
            s.skill = r.u32()?;
        }
        s.basic_attack = b & story_bits::BASIC_ATTACK != 0;
        for h in read_packs::<StoryHookData>(r, ctx)? {
            s.add_hook(h);
        }
        Ok(s)
    }
}

/// The effects and behaviors a hit shows on the attacker, its target and its weapon.
// D2-15905, D2-15906, D2-15908, D2-15909
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FxAndBehaviorInfo<B> {
    pub attacker_fx: u32,
    pub target_fx: u32,
    pub weapon_fx: u32,
    pub target_behavior: B,
    // D2-15902
    pub weapon_behavior: B,
}

impl<B: Behavior> FxAndBehaviorInfo<B> {
    /// No effects and no behaviors.
    // D2-15907
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<B: Behavior> Pack for FxAndBehaviorInfo<B> {
    /// The three effects, then the target's behavior and the weapon's.
    // D2-15903
    fn pack(&self, w: &mut Writer) {
        w.u32(self.attacker_fx);
        w.u32(self.target_fx);
        w.u32(self.weapon_fx);
        self.target_behavior.pack(w);
        self.weapon_behavior.pack(w);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            attacker_fx: r.u32()?,
            target_fx: r.u32()?,
            weapon_fx: r.u32()?,
            target_behavior: B::unpack(r, ctx)?,
            weapon_behavior: B::unpack(r, ctx)?,
        })
    }
}

/// A missile as the game's rules make it: what object it is, which holding place on the attacker
/// it starts in, how it flies, what it aims at, what it passes through, whether it is the thrown
/// weapon itself, and whether it looks like the weapon.
// D2-15881, D2-15882, D2-15890, D2-15897, D2-15898
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MissileInfo {
    /// The record the missile object is made from.
    pub entity: u32,
    /// Where on the attacker it is held before it is fired; none makes it at the moment of firing.
    pub holding_location: u32,
    pub params: MissileParameters,
    // D2-15885
    pub targeting: MissileTargetingParameters,
    // D2-15887
    pub collision: CollisionSettings,
    // D2-15886
    pub thrown: bool,
    // D2-15889
    pub clone: bool,
}

impl MissileInfo {
    // D2-15883
    pub fn set_thrown(&mut self, on: bool) {
        self.thrown = on;
    }

    // D2-15888
    pub fn set_clone(&mut self, on: bool) {
        self.clone = on;
    }

    // D2-15899
    pub fn set_holding_location(&mut self, location: u32) {
        self.holding_location = location;
    }

    // D2-15893
    pub fn set_missile_parameters(&mut self, p: &MissileParameters) {
        self.params = *p;
    }

    // D2-15894
    pub fn set_targeting_parameters(&mut self, t: &MissileTargetingParameters) {
        self.targeting = *t;
    }

    /// Takes a copy: the bits, the three physics-type masks, both object lists, the objects
    /// passed through for now and the least impact speeds.
    // D2-15896, D2-15895
    pub fn set_collision_parameters(&mut self, c: &CollisionSettings) {
        self.collision = c.clone();
    }
}

impl Pack for MissileInfo {
    /// The entity, the holding place, the flight, the aim, the collision settings, then thrown and
    /// clone.
    // D2-15891
    fn pack(&self, w: &mut Writer) {
        w.u32(self.entity);
        w.u32(self.holding_location);
        self.params.pack(w);
        self.targeting.pack(w);
        self.collision.pack(w);
        w.bool32(self.thrown);
        w.bool32(self.clone);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            entity: r.u32()?,
            holding_location: r.u32()?,
            params: MissileParameters::unpack(r, ctx)?,
            targeting: MissileTargetingParameters::unpack(r, ctx)?,
            collision: CollisionSettings::unpack(r, ctx)?,
            thrown: r.bool32()?,
            clone: r.bool32()?,
        })
    }
}
