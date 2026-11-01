//! Playing a story on the attacker's body: the attacker's behavior starts it, each animation hook
//! of the attack lets the story hooks waiting for it run, and the behavior's end (or its failure)
//! finishes and clears it.

use auberean_assets::Context;
use auberean_dat::reader::Result;

use super::{kind, Behavior, FxAndBehaviorInfo, MissileInfo, PhysicsStory, StoryHookData};
use crate::collision::CollisionSettings;
use crate::missile::{MissileParameters, MissileTargetingParameters};
use crate::pack::{Pack, Reader, Writer};
use crate::target::NO_OBJECT;

/// The animation hook type that moves a story on.
pub const STORY_HOOK: u32 = 0x4000_0002;
/// The slider a thrown weapon is faded out with while its missile flies, and back in after.
pub const FADE_SLIDER: u32 = 0x4000_0015;
/// How long a missile object lives, seconds.
pub const MISSILE_TIMEOUT: f64 = 8.0;

/// The world as a story on one body sees it: the body, its game object, and the objects the story
/// names.
pub trait StoryHost<B> {
    /// Whether the body is the player's own.
    fn is_the_player(&self) -> bool;
    /// Whether the body has a game object to ask the game's rules through.
    fn has_game_object(&self) -> bool;
    /// The player's attack with this client context has begun.
    fn attack_started(&mut self, context: u32);
    /// The player's attack with this client context is over.
    fn attack_ended(&mut self, context: u32);
    /// The effects and behaviors a hit shows, by the game's rules: false for none.
    fn hit_effects(
        &mut self,
        skill: u32,
        attack_hook: u32,
        result: u32,
        info: &mut FxAndBehaviorInfo<B>,
    ) -> bool;
    /// The missile an attack fires, by the game's rules: false for none.
    fn missile_info(
        &mut self,
        skill: u32,
        attack_hook: u32,
        attacker: u64,
        target: u64,
        info: &mut MissileInfo,
    ) -> bool;
    /// Whether a skill harms whom it is used on.
    fn is_skill_harmful(&mut self, skill: u32) -> bool;
    /// Plays a behavior on the body: false when it cannot.
    fn do_behavior(&mut self, behavior: &B) -> bool;
    /// Plays an effect on the body.
    fn do_fx(&mut self, fx: u32, intensity: f32);

    /// Whether an object is in the object table.
    fn object_exists(&self, object: u64) -> bool;
    fn object_behavior(&mut self, object: u64, behavior: &B);
    fn object_fx(&mut self, object: u64, fx: u32, intensity: f32);
    /// Moves one of an object's sliders to `value` over `seconds`.
    fn object_slider(&mut self, object: u64, slider: u32, value: f32, seconds: f32);
    /// Whether an object has a game object.
    fn object_has_game_object(&self, object: u64) -> bool;
    /// Shows a hit on the target's game object: who struck whom with what skill, the result, and
    /// the changes to health and player-killer damage on both sides.
    #[allow(clippy::too_many_arguments)]
    fn show_damage(
        &mut self,
        attacker: u64,
        target: u64,
        skill: u32,
        result: u32,
        target_health: i32,
        attacker_health: i32,
        target_pk: i32,
        attacker_pk: i32,
    );
    /// The player's own object id.
    fn player_id(&self) -> u64;
    /// Tells the target's game object who last attacked it.
    fn notice_last_attacker(&mut self, target: u64, attacker: u64);

    /// Removes an object from the world.
    fn destroy_object(&mut self, object: u64);
    /// The record an object was made from.
    fn object_data_id(&self, object: u64) -> u32;
    /// Makes an object from a record, to be removed after `seconds`: its id, or none.
    fn create_temporary(&mut self, data_id: u32, seconds: f64) -> Option<u64>;
    /// Gives `to` the look `from` has.
    fn copy_look(&mut self, from: u64, to: u64);
    fn set_collision_state(&mut self, object: u64, c: &CollisionSettings);
    /// Starts a missile's guidance.
    fn missile_initialize(
        &mut self,
        object: u64,
        params: &MissileParameters,
        targeting: &MissileTargetingParameters,
    );
    /// Puts an object in the body's hand at a holding place.
    fn hold(&mut self, object: u64, location: u32);
    /// Lets a missile go: placed in the world and fired.
    fn missile_place_and_fire(&mut self, object: u64);
}

/// The story playing on one body: the story, the hook it is at, how many animation hooks have
/// passed, the missiles made and waiting to be fired, the weapons faded out while their missiles
/// fly, and the context of the last behavior it started.
// D2-07202, D2-07204
#[derive(Clone, Debug, PartialEq)]
pub struct StoryPlayer<B> {
    pub story: Option<PhysicsStory<B>>,
    /// The next hook to run, an index into the story's hooks.
    pub hook: usize,
    pub anim_hooks: u32,
    pub pending_missiles: Vec<u64>,
    pub faded: Vec<u64>,
    pub context: u32,
}

impl<B: Behavior> Default for StoryPlayer<B> {
    // D2-07201
    fn default() -> Self {
        Self {
            story: None,
            hook: 0,
            anim_hooks: 0,
            pending_missiles: Vec::new(),
            faded: Vec::new(),
            context: 1,
        }
    }
}

impl<B: Behavior> StoryPlayer<B> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn playing(&self) -> Option<&PhysicsStory<B>> {
        self.story.as_ref().filter(|s| !s.is_null())
    }

    fn at_end(&self) -> bool {
        self.story
            .as_ref()
            .is_none_or(|s| self.hook >= s.hooks.len())
    }

    /// Makes the missile a hook fires: a copy of the weapon's look when the missile clones it and
    /// the weapon is there, else the missile's own record; then its collision settings and its
    /// guidance.
    // D2-07195
    pub fn create_missile(
        host: &mut dyn StoryHost<B>,
        hook: &StoryHookData,
        info: &MissileInfo,
    ) -> Option<u64> {
        let mut made = None;
        if info.clone && host.object_exists(hook.weapon) {
            let id = host.object_data_id(hook.weapon);
            if let Some(m) = host.create_temporary(id, MISSILE_TIMEOUT) {
                host.copy_look(hook.weapon, m);
                made = Some(m);
            }
        }
        let m = match made {
            Some(m) => m,
            None => host.create_temporary(info.entity, MISSILE_TIMEOUT)?,
        };
        host.set_collision_state(m, &info.collision);
        host.missile_initialize(m, &info.params, &info.targeting);
        Some(m)
    }

    /// Fires the current hook's missile: one made now when the rules hold it nowhere, else the
    /// first made earlier. A missile that cannot be made is not fired (the client itself would
    /// fault).
    // D2-07199
    pub fn fire_missile(&mut self, host: &mut dyn StoryHost<B>) {
        let mut info = MissileInfo::default();
        if !host.has_game_object() {
            return;
        }
        let Some(story) = self.story.as_ref() else {
            return;
        };
        let Some(hook) = story.hooks.get(self.hook).copied() else {
            return;
        };
        let (skill, attacker) = (story.skill, story.attacker);
        if !host.missile_info(skill, hook.attack_hook, attacker, hook.target, &mut info) {
            return;
        }
        let missile = if info.holding_location == 0 {
            Self::create_missile(host, &hook, &info)
        } else {
            if self.pending_missiles.is_empty() {
                return;
            }
            let id = self.pending_missiles.remove(0);
            host.object_exists(id).then_some(id)
        };
        if let Some(m) = missile {
            host.missile_place_and_fire(m);
        }
    }

    /// Ends the story: the missiles not fired are removed, the faded weapons fade back in over
    /// half a second, the player's attack is over, and the story is emptied.
    // D2-07200
    pub fn cleanup_story(&mut self, host: &mut dyn StoryHost<B>) {
        let Some(context) = self.playing().map(|s| s.client_context) else {
            return;
        };
        for id in self.pending_missiles.drain(..) {
            host.destroy_object(id);
        }
        for id in self.faded.drain(..) {
            if host.object_exists(id) {
                host.object_slider(id, FADE_SLIDER, 1.0, 0.5);
            }
        }
        if host.is_the_player() && host.has_game_object() {
            host.attack_ended(context);
        }
        self.story = Some(PhysicsStory::default());
    }

    /// Makes the missiles of every fire hook that the rules hold in the attacker's hand, and puts
    /// them there; a thrown weapon is faded out meanwhile. Only once a story, while none wait.
    // D2-07205
    pub fn create_and_parent_missiles(&mut self, host: &mut dyn StoryHost<B>) {
        if !self.pending_missiles.is_empty() {
            return;
        }
        let Some(story) = self.playing() else {
            return;
        };
        let (hooks, skill, attacker) = (story.hooks.clone(), story.skill, story.attacker);
        for hook in hooks.iter().filter(|h| h.kind == kind::FIRE_MISSILE) {
            let mut info = MissileInfo::default();
            if !host.has_game_object()
                || !host.missile_info(skill, hook.attack_hook, attacker, hook.target, &mut info)
                || info.holding_location == 0
            {
                continue;
            }
            let Some(m) = Self::create_missile(host, hook, &info) else {
                continue;
            };
            host.hold(m, info.holding_location);
            if info.thrown && host.object_exists(hook.weapon) {
                host.object_slider(hook.weapon, FADE_SLIDER, 0.0, 0.0);
                self.faded.push(hook.weapon);
            }
            self.pending_missiles.push(m);
        }
    }

    /// Runs one hook. A hit shows the rules' effects and behaviors on the attacker, the target and
    /// the weapon, then the damage on the target, and tells the target it was attacked when it is
    /// the player and the skill harms. The other kinds make or fire missiles.
    // D2-07206
    pub fn do_story_hook(&mut self, host: &mut dyn StoryHost<B>, hook: &StoryHookData) {
        let Some(story) = self.story.as_ref() else {
            return;
        };
        let (skill, attacker, basic) = (story.skill, story.attacker, story.basic_attack);
        let mut info = FxAndBehaviorInfo::<B>::default();
        let game_object = host.has_game_object();
        if game_object && host.hit_effects(skill, hook.attack_hook, hook.attack_result, &mut info) {
            if info.attacker_fx != 0 {
                host.do_fx(info.attacker_fx, 1.0);
            }
            if hook.target != NO_OBJECT && host.object_exists(hook.target) {
                if info.target_behavior.is_valid() {
                    host.object_behavior(hook.target, &info.target_behavior);
                }
                if info.target_fx != 0 {
                    host.object_fx(hook.target, info.target_fx, 1.0);
                }
            }
            if hook.weapon != NO_OBJECT && host.object_exists(hook.weapon) {
                if info.weapon_behavior.is_valid() {
                    host.object_behavior(hook.weapon, &info.weapon_behavior);
                }
                if info.weapon_fx != 0 {
                    host.object_fx(hook.weapon, info.weapon_fx, 1.0);
                }
            }
        }
        match hook.kind {
            kind::DAMAGE => {
                if !host.object_has_game_object(hook.target) {
                    return;
                }
                let shown_skill = if basic { 0 } else { skill };
                host.show_damage(
                    attacker,
                    hook.target,
                    shown_skill,
                    hook.attack_result,
                    hook.target_health_change,
                    hook.attacker_health_change,
                    hook.target_pk_damage,
                    hook.attacker_pk_damage,
                );
                // The harm is asked of the attacker's own game object; without one the client
                // itself would fault here.
                if hook.target == host.player_id()
                    && skill != 0
                    && game_object
                    && host.is_skill_harmful(skill)
                {
                    host.notice_last_attacker(hook.target, attacker);
                }
            }
            kind::CREATE_MISSILES => self.create_and_parent_missiles(host),
            kind::FIRE_MISSILE => self.fire_missile(host),
            _ => {}
        }
    }

    /// Runs, in order, every hook whose animation hook has passed.
    // D2-07207
    pub fn do_current_story_hooks(&mut self, host: &mut dyn StoryHost<B>) {
        loop {
            let Some(story) = self.playing() else {
                return;
            };
            let Some(hook) = story.hooks.get(self.hook).copied() else {
                return;
            };
            if hook.anim_hook > self.anim_hooks {
                return;
            }
            self.do_story_hook(host, &hook);
            self.hook += 1;
        }
    }

    /// Runs every hook left, one animation hook at a time, then cleans up.
    // D2-07208
    pub fn finish_story(&mut self, host: &mut dyn StoryHost<B>) {
        while self.playing().is_some() && !self.at_end() {
            self.anim_hooks = self.anim_hooks.wrapping_add(1);
            self.do_current_story_hooks(host);
        }
        self.cleanup_story(host);
    }

    /// Plays a new story, finishing the one playing. The player's attack begins; the attacker's
    /// behavior is played under a new context (never 0) and the hooks waiting for no animation hook
    /// run at once. A story without an attacker behavior is finished at once; one whose behavior
    /// cannot play is cleaned up.
    // D2-07209
    pub fn do_story(&mut self, host: &mut dyn StoryHost<B>, story: &PhysicsStory<B>) {
        if self.playing().is_some() {
            self.finish_story(host);
        }
        self.story = Some(story.clone());
        if host.is_the_player() && host.has_game_object() {
            host.attack_started(story.client_context);
        }
        self.hook = 0;
        self.anim_hooks = 0;
        let mut behavior = story.attacker_behavior.clone();
        if !behavior.is_valid() {
            self.finish_story(host);
            return;
        }
        self.context = self.context.wrapping_add(1);
        if self.context == 0 {
            self.context = 1;
        }
        behavior.set_context(self.context);
        if let Some(s) = self.story.as_mut() {
            s.attacker_behavior.set_context(self.context);
        }
        if !host.do_behavior(&behavior) {
            self.cleanup_story(host);
            return;
        }
        self.do_current_story_hooks(host);
    }

    /// An animation hook of the attack: a story hook passes the next.
    // D2-07210
    pub fn handle_anim_hook(&mut self, host: &mut dyn StoryHost<B>, hook_type: u32) {
        if hook_type != STORY_HOOK || self.playing().is_none() {
            return;
        }
        self.anim_hooks = self.anim_hooks.wrapping_add(1);
        self.do_current_story_hooks(host);
    }

    /// The attacker's behavior ended (`failed` 0) or failed: the story is finished, or cleaned
    /// up, when it is this story's behavior.
    // D2-07211
    pub fn handle_behavior_done(&mut self, host: &mut dyn StoryHost<B>, context: u32, failed: u32) {
        if context != self.context {
            return;
        }
        if failed == 0 {
            self.finish_story(host);
        } else {
            self.cleanup_story(host);
        }
    }

    /// Its binary form: the story; with no story there is nothing to write (false).
    // D2-07203
    pub fn write(&self, w: &mut Writer) -> bool {
        match &self.story {
            Some(s) => {
                s.pack(w);
                true
            }
            None => false,
        }
    }

    pub fn read(&mut self, r: &mut Reader<'_>, ctx: &Context) -> Result<()> {
        self.story = Some(PhysicsStory::unpack(r, ctx)?);
        Ok(())
    }
}
