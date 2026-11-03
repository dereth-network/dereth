//! The server's events for the objects' bodies, played on them: a behavior played or stopped, an
//! effect played or stopped, and the combat stories.
//!
//! **A story** the server tells on an object plays on that object's body (the attacker's, or a
//! struck object's for an arrival it tells): the body's story player
//! ([`auberean_physics::story::StoryPlayer`], made the first time a story is told on the body)
//! plays the attacker's behavior on its figure under a context of its own, and runs the story's
//! hooks as the figure's animation reaches the hooks they wait for. Each figure's animation hooks
//! go to its body's story player in the frame they fire, after what the figure finished that frame
//! (a behavior's end finishes its story, its failure cleans it up). The story asks the world
//! through the scene ([`Stage`]): the effects and behaviors a hit shows on the attacker, the target
//! and the weapon and the damage the target shows, by the objects' scripts (the runtime's answers);
//! the missiles it makes, holds and fires ([`crate::temporaries`]); the player's scripts hearing
//! that its attack began and ended, and who last attacked it.
//!
//! **A behavior** asked of a body plays on its figure; one that is passed on goes to what the body
//! holds as well. **An effect** plays on the body's figure (its particles) and moves the camera
//! when it has a camera part.

use auberean_animation::host::{Completion, Waited};
use auberean_animation::track::Params;
use auberean_assets::Context;
use auberean_client_sdk::protocol::table::Table;
use auberean_client_sdk::protocol::types::{
    BehaviorParams as WireBehavior, PhysicsStory as WireStory,
};
use auberean_client_sdk::protocol::wire::{Field, Reader as WireReader, Writer as WireWriter};
use auberean_client_sdk::protocol::ServerMessage;
use auberean_client_sdk::runtime::combat::{Damage, HitEffects};
use auberean_client_sdk::Client;
use auberean_dat::reader::Result;
use auberean_physics::collision::CollisionSettings;
use auberean_physics::missile::{MissileParameters, MissileTargetingParameters};
use auberean_physics::pack::{Pack, Reader, Writer};
use auberean_physics::story::{
    Behavior, FxAndBehaviorInfo, MissileInfo, PhysicsStory, StoryHost, StoryPlayer,
};
use auberean_physics::target::NO_OBJECT;

use crate::Scene;

/// What the stories did on the scene's bodies, counted: the stories told on a body here and on
/// none, the attackers' behaviors their figures took and refused, the hooks run, the hits whose
/// damage was shown, the player's attacks begun and ended, and the missiles made, held, fired and
/// in flight, and of those the player's own made, held and in flight.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoryTally {
    pub stories: u64,
    pub without_body: u64,
    pub behaviors: u64,
    pub refused: u64,
    pub hooks: u64,
    pub hits: u64,
    pub attacks_started: u64,
    pub attacks_ended: u64,
    pub missiles_made: u64,
    pub missiles_held: u64,
    pub missiles_fired: u64,
    pub missiles_flying: u64,
    /// Missiles that went a meter from where they were put in the world.
    pub missiles_travelled: u64,
    /// Missiles that stuck in what they struck.
    pub missiles_stuck: u64,
    /// Missiles that went off on their target.
    pub missiles_hit_target: u64,
    pub players_made: u64,
    pub players_held: u64,
    pub players_flying: u64,
    /// The player's held missiles let go of in front of the player, higher than a meter over its
    /// feet.
    pub players_let_go_ahead: u64,
    /// The player's missiles that went off on their target.
    pub players_hit_target: u64,
    /// Behaviors the server asked that carry an emote, and those whose emote the object's
    /// scripts did not take.
    pub emotes_asked: u64,
    pub emotes_refused: u64,
}

/// A behavior as a story carries it: what the figure plays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Act(pub Params);

impl Behavior for Act {
    // D2-14909
    fn is_valid(&self) -> bool {
        self.0.is_valid()
    }

    fn set_context(&mut self, context: u32) {
        self.0.context = context;
    }
}

impl Pack for Act {
    /// The behavior's binary form, as the server sends it.
    fn pack(&self, w: &mut Writer) {
        let mut out = WireWriter::default();
        wire_of(&self.0).write(&mut out);
        w.bytes(&out.finish());
    }

    fn unpack(r: &mut Reader<'_>, _ctx: &Context) -> Result<Self> {
        let mut wire = WireReader::new(r.rest());
        let b = match WireBehavior::read(&mut wire) {
            Ok(b) => b,
            Err(e) => return r.fail(e.to_string()),
        };
        r.skip(wire.pos())?;
        Ok(Self(params_of_wire(&b)))
    }
}

/// A behavior as the server sends it, as the figure takes it: the fields its bits name, the rest
/// at their defaults.
#[must_use]
pub fn params_of_wire(w: &WireBehavior) -> Params {
    let has = |bit: u32| w.bits & bit != 0;
    let mut p = Params::default();
    if has(WireBehavior::BEHAVIOR) {
        p.behavior = w.behavior;
    }
    if has(WireBehavior::TIME_SCALE) {
        p.time_scale = w.time_scale;
    }
    if has(WireBehavior::MODE) {
        p.mode = w.mode;
    }
    if has(WireBehavior::HOLD_CYCLES) {
        p.hold_cycles = w.hold_cycles;
    }
    if has(WireBehavior::FX_SCRIPT) {
        p.fx_script = w.fx_script;
    }
    if has(WireBehavior::FX) {
        p.fx = w.fx;
    }
    if has(WireBehavior::TARGET) {
        p.target = w.target;
    }
    if has(WireBehavior::CAMERA) {
        p.camera_target = w.camera_target;
        p.camera_behavior = w.camera_behavior;
    }
    if has(WireBehavior::CLONE) {
        p.clone_visual = w.clone_visual;
        p.clone_appearance = w.clone_appearance;
        p.clone_strengths.clone_from(&w.clone_strengths.entries);
    }
    if has(WireBehavior::IMPULSE) {
        p.impulse = w.impulse;
    }
    p.set_words(w.bits, &w.words);
    if has(WireBehavior::CONTEXT) {
        p.context = w.context;
    }
    p
}

/// A behavior as the server would send it: the bits of the fields that differ from a new one's,
/// and those fields.
#[must_use]
pub fn wire_of(p: &Params) -> WireBehavior {
    use auberean_animation::track::bits;
    let mut words = [0u32; 13];
    for (i, bit) in bits::WORDS.iter().enumerate() {
        let on = |b: bool| u32::from(b);
        words[i] = match *bit {
            bits::EARLY_CALLBACK => on(p.early_callback),
            bits::MOVE_TO_CANCELS => on(p.move_to_cancels),
            bits::CAMERA_PARENT => on(p.camera_parent),
            bits::CAMERA_TARGETS => on(p.camera_targets),
            bits::CAMERA_HOLD => on(p.camera_hold),
            bits::CAMERA_RESTORE => on(p.camera_restore),
            bits::FADE_CHILDREN => on(p.fade_children),
            bits::PROPAGATE => on(p.propagate),
            bits::LOCK_ACTIONS => on(p.lock_actions),
            bits::SEND_EVENT => on(p.send_event),
            bits::LEAVE_IDLE_ALONE => on(p.leave_idle_alone),
            bits::DESTROY_ON_COMPLETION => on(p.destroy_on_completion),
            _ => p.emote,
        };
    }
    WireBehavior {
        bits: p.pack_bits(),
        behavior: p.behavior,
        time_scale: p.time_scale,
        mode: p.mode,
        hold_cycles: p.hold_cycles,
        fx_script: p.fx_script,
        fx: p.fx,
        target: p.target,
        camera_target: p.camera_target,
        camera_behavior: p.camera_behavior,
        clone_visual: p.clone_visual,
        clone_appearance: p.clone_appearance,
        clone_strengths: Table {
            entries: p.clone_strengths.clone(),
            ..Table::default()
        },
        impulse: p.impulse,
        words,
        context: p.context,
    }
}

/// The story the server told, as the body plays it: read from its binary form, its hooks in the
/// order of the animation hooks they wait for. `None` for one that does not read back.
#[must_use]
pub fn story_of(w: &WireStory) -> Option<PhysicsStory<Act>> {
    let mut out = WireWriter::default();
    w.write(&mut out);
    let bytes = out.finish();
    PhysicsStory::<Act>::unpack(&mut Reader::new(&bytes), &Context::empty()).ok()
}

/// What a hit shows, as the scripts answered: its effects, and its behaviors as the figures play
/// them.
fn info_of(h: &HitEffects) -> FxAndBehaviorInfo<Act> {
    let act = |b: &Option<auberean_client_sdk::script::native_object::BehaviorParams>| {
        b.as_ref()
            .map_or_else(Act::default, |b| Act(crate::params_of(b)))
    };
    FxAndBehaviorInfo {
        attacker_fx: h.attacker_fx,
        target_fx: h.target_fx,
        weapon_fx: h.weapon_fx,
        target_behavior: act(&h.target_behavior),
        weapon_behavior: act(&h.weapon_behavior),
    }
}

/// The world as a story on one body sees it: the scene and the client, and the body.
pub(crate) struct Stage<'s, 'w> {
    pub scene: &'s mut Scene<'w>,
    pub client: &'s mut Client,
    pub body: u64,
}

impl StoryHost<Act> for Stage<'_, '_> {
    fn is_the_player(&self) -> bool {
        self.scene
            .player
            .as_ref()
            .is_some_and(|p| p.id == self.body)
    }

    fn has_game_object(&self) -> bool {
        self.client.model.object(self.body).is_some()
    }

    fn attack_started(&mut self, context: u32) {
        self.scene.combat.attacks_started += 1;
        self.client.special_attack_started(context);
    }

    fn attack_ended(&mut self, context: u32) {
        self.scene.combat.attacks_ended += 1;
        self.client.special_attack_ended(context);
    }

    fn hit_effects(
        &mut self,
        skill: u32,
        attack_hook: u32,
        result: u32,
        info: &mut FxAndBehaviorInfo<Act>,
    ) -> bool {
        self.scene.combat.hooks += 1;
        match self
            .client
            .hit_effects(self.body, skill, attack_hook, result)
        {
            Some((_, h)) => {
                *info = info_of(&h);
                true
            }
            None => false,
        }
    }

    fn missile_info(
        &mut self,
        skill: u32,
        attack_hook: u32,
        attacker: u64,
        target: u64,
        info: &mut MissileInfo,
    ) -> bool {
        match self
            .client
            .missile_info(self.body, skill, attack_hook, attacker, target)
        {
            Some((_, m)) => {
                *info = m;
                true
            }
            None => false,
        }
    }

    fn is_skill_harmful(&mut self, skill: u32) -> bool {
        self.client
            .is_skill_harmful(self.body, skill)
            .is_some_and(|(_, harms)| harms)
    }

    fn do_behavior(&mut self, behavior: &Act) -> bool {
        let took = self.scene.play_behavior(self.body, &behavior.0);
        if took {
            self.scene.combat.behaviors += 1;
        } else {
            self.scene.combat.refused += 1;
        }
        took
    }

    fn do_fx(&mut self, fx: u32, intensity: f32) {
        self.scene.do_fx(self.body, fx, intensity);
    }

    fn object_exists(&self, object: u64) -> bool {
        self.scene.has_body(object)
    }

    // D2-06076
    fn object_behavior(&mut self, object: u64, behavior: &Act) {
        self.scene.play_behavior(object, &behavior.0);
    }

    fn object_fx(&mut self, object: u64, fx: u32, intensity: f32) {
        self.scene.do_fx(object, fx, intensity);
    }

    fn object_slider(&mut self, object: u64, slider: u32, value: f32, seconds: f32) {
        if let Some(f) = self.scene.figure_mut(object) {
            f.sliders.do_slider(0, slider, value, seconds);
        }
    }

    fn object_has_game_object(&self, object: u64) -> bool {
        self.client.model.object(object).is_some()
    }

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
    ) {
        self.scene.combat.hits += 1;
        self.client.show_damage(&Damage {
            attacker,
            target,
            skill,
            result,
            target_health,
            attacker_health,
            target_pk,
            attacker_pk,
        });
    }

    fn player_id(&self) -> u64 {
        self.client.model.player.unwrap_or(NO_OBJECT)
    }

    fn notice_last_attacker(&mut self, _target: u64, attacker: u64) {
        self.client.notice_last_attacker(attacker);
    }

    fn destroy_object(&mut self, object: u64) {
        self.scene.destroy_temporary(self.client, object);
    }

    fn object_data_id(&self, object: u64) -> u32 {
        self.scene.data_id_of(self.client, object)
    }

    fn create_temporary(&mut self, data_id: u32, seconds: f64) -> Option<u64> {
        let made = self.scene.create_temporary(self.client, data_id, seconds);
        self.scene.combat.missiles_made += u64::from(made.is_some());
        if let Some(t) = made.and_then(|m| self.scene.temporaries.get_mut(m)) {
            t.maker = self.body;
            if self.is_the_player() {
                self.scene.combat.players_made += 1;
            }
        }
        made
    }

    fn copy_look(&mut self, from: u64, to: u64) {
        self.scene.copy_look(self.client, from, to);
    }

    fn set_collision_state(&mut self, object: u64, c: &CollisionSettings) {
        self.scene.set_body_collision_state(object, c);
    }

    fn missile_initialize(
        &mut self,
        object: u64,
        params: &MissileParameters,
        targeting: &MissileTargetingParameters,
    ) {
        self.scene.missile_initialize(object, params, targeting);
    }

    fn hold(&mut self, object: u64, location: u32) {
        self.scene.hold_temporary(object, self.body, location);
        let held = self
            .scene
            .temporaries
            .get(object)
            .is_some_and(|t| t.holder.is_some());
        self.scene.combat.missiles_held += u64::from(held);
        if held && self.is_the_player() {
            self.scene.combat.players_held += 1;
        }
    }

    fn missile_place_and_fire(&mut self, object: u64) {
        self.scene.combat.missiles_fired += 1;
        self.scene.fire_temporary(object);
    }
}

/// What the server asks of a body itself: a push (away from an object, or as given), an effect
/// script, the end of a layer or of behaviors, what the ground does to its motion.
// D2-05844, D2-06117, D2-05841, D2-05842, D2-05843, D2-05857
fn recv_on_body(b: &mut auberean_physics::body::Body, m: &ServerMessage) {
    match m {
        ServerMessage::Push(e) => {
            b.add_impulse(crate::pv(e.action.direction));
        }
        ServerMessage::DoFxScript(e) => {
            b.receive_do_fx_script(e.action.0);
        }
        ServerMessage::EndLayer(e) => b.receive_stop_layer(e.action.0),
        ServerMessage::StopAllBehaviors(_) => b.receive_stop_all_server_behaviors(),
        ServerMessage::StopMoveCancelsBehaviors(_) => {
            b.receive_stop_move_to_cancels_behaviors();
        }
        ServerMessage::HoverHeight(e) => b.set_hover_height(e.action.0),
        ServerMessage::GlobalModifier(e) => b.set_global_modifier(e.action.0),
        ServerMessage::TerrainChange(e) => {
            b.add_terrain_modifier(e.action.terrain, e.action.modifier);
        }
        ServerMessage::ClearTerrainChanges(_) => b.clear_terrain_modifiers(),
        _ => {}
    }
}

impl Scene<'_> {
    /// Runs `f` on body `body`'s story player (made when it has none) with the world as the story
    /// sees it.
    fn with_story(
        &mut self,
        client: &mut Client,
        body: u64,
        f: impl FnOnce(&mut StoryPlayer<Act>, &mut Stage<'_, '_>),
    ) {
        let mut player = self.stories.remove(&body).unwrap_or_default();
        let mut stage = Stage {
            scene: self,
            client,
            body,
        };
        f(&mut player, &mut stage);
        self.stories.insert(body, player);
    }

    /// A story told on object `body`: played on its body, when it has one.
    // D2-04020, D2-06093, D2-06235
    pub(crate) fn do_story(&mut self, client: &mut Client, body: u64, story: &WireStory) {
        if !self.has_body(body) {
            return;
        }
        self.combat.stories += 1;
        let Some(story) = story_of(story) else {
            tracing::debug!(
                body = format_args!("{body:#x}"),
                "a story that does not read back"
            );
            return;
        };
        self.with_story(client, body, |p, stage| p.do_story(stage, &story));
    }

    /// A behavior the server asked of object `id`'s body: played with the context 1, unless it
    /// carries an emote the object's own scripts do not take (with no scripts running, every one
    /// is taken). Whether it was played.
    // D2-06363
    pub(crate) fn recv_do_behavior(
        &mut self,
        client: &mut Client,
        id: u64,
        mut params: Params,
    ) -> bool {
        params.context = 1;
        if params.emote != 0 {
            self.combat.emotes_asked += 1;
        }
        if params.emote != 0 && client.handle_weenie_emote(id, params.emote) == Some(false) {
            self.combat.emotes_refused += 1;
            return false;
        }
        self.play_behavior(id, &params);
        true
    }

    /// What a body's figure finished this frame, then the animation hooks it fired, handed to the
    /// body's story player: a behavior's end or failure, and each hook in turn.
    // D2-06204, D2-06403
    pub(crate) fn story_heard(
        &mut self,
        client: &mut Client,
        body: u64,
        done: &[Completion],
        hooks: &[u32],
    ) {
        if !self.stories.contains_key(&body) {
            return;
        }
        for c in done.iter().filter(|c| c.waited == Waited::Behavior) {
            self.with_story(client, body, |p, stage| {
                p.handle_behavior_done(stage, c.context, c.error);
            });
        }
        for &kind in hooks {
            self.with_story(client, body, |p, stage| p.handle_anim_hook(stage, kind));
        }
    }

    /// The server's events for bodies, in order: stories told, behaviors played and stopped,
    /// effects played and stopped, moves ordered, and the scales of a body's speed, acceleration
    /// and jump. Those for an object with no body here are given back.
    // D2-04016, D2-04037, D2-04038, D2-05848, D2-05860, D2-05862, D2-04018, D2-04034, D2-05883
    // D2-04035, D2-04029, D2-04032
    pub(crate) fn play_server_events(
        &mut self,
        client: &mut Client,
        events: Vec<ServerMessage>,
    ) -> Vec<ServerMessage> {
        let mut waiting = Vec::new();
        for m in events {
            // D2-06167
            if let ServerMessage::PrivateFx(f) = &m {
                if let Some(id) = self.player.as_ref().map(|p| p.id) {
                    for fx in &f.0 {
                        self.do_fx(id, *fx, 1.0);
                    }
                }
                continue;
            }
            let Some(id) = auberean_client_sdk::model::events::subject(&m).map(|s| s.id) else {
                continue;
            };
            if !self.has_body(id) {
                waiting.push(m);
                continue;
            }
            match m {
                ServerMessage::DoStory(e) => self.do_story(client, id, &e.action),
                ServerMessage::PlayBehavior(e) => {
                    self.recv_do_behavior(client, id, params_of_wire(&e.action));
                }
                // D2-05848
                ServerMessage::StopBehavior(e) => {
                    let anims = &self.anims;
                    if let Some(f) = crate::figure_in(
                        &mut self.player,
                        &mut self.others,
                        &mut self.held,
                        &mut self.temporaries,
                        id,
                    ) {
                        f.stop_behavior(anims, e.action.0);
                    }
                }
                // D2-05884
                ServerMessage::DoMode(e) => {
                    self.do_mode(id, e.action.0);
                }
                ServerMessage::SetMode(e) => {
                    let anims = &self.anims;
                    if let Some(f) = crate::figure_in(
                        &mut self.player,
                        &mut self.others,
                        &mut self.held,
                        &mut self.temporaries,
                        id,
                    ) {
                        f.set_mode(anims, e.action.0);
                    }
                }
                // D2-05860
                ServerMessage::DoFx(e) => self.do_fx(id, e.action.fx, e.action.scalar),
                ServerMessage::StopFx(e) => self.stop_fx(id, e.action.0),
                ServerMessage::MoveTo(e) => self.recv_move_to(id, &e.action),
                ServerMessage::VelocityScale(e) => {
                    if let Some(b) = self.body_mut(id) {
                        b.set_velocity_scale(e.action.0);
                    }
                }
                ServerMessage::AccelerationScale(e) => {
                    if let Some(b) = self.body_mut(id) {
                        b.set_acceleration_scale(e.action.0);
                    }
                }
                ServerMessage::JumpScale(e) => {
                    if let Some(b) = self.body_mut(id) {
                        b.set_jump_scale(e.action.0);
                    }
                }
                ServerMessage::LookAt(e) => {
                    self.look_at(id, e.action.0);
                }
                ServerMessage::TurnHead(e) => self.look_dir(id, e.action.x, e.action.y),
                ServerMessage::MoveSlider(e) => self.recv_do_slider(id, &e.action),
                // D2-05886
                ServerMessage::LetGoBehavior(e) => {
                    if let Some(b) = self.body_mut(id) {
                        b.set_animation_active(true);
                    }
                    if e.action.params.mode != 0 {
                        self.do_mode(id, e.action.params.mode);
                    }
                    let anims = &self.anims;
                    if let Some(f) = crate::figure_in(
                        &mut self.player,
                        &mut self.others,
                        &mut self.held,
                        &mut self.temporaries,
                        id,
                    ) {
                        f.release_behavior(
                            anims,
                            e.action.behavior,
                            &params_of_wire(&e.action.params),
                        );
                    }
                }
                ServerMessage::DoTargetedFx(e) => {
                    self.do_targeted_fx(id, e.action.fx, e.action.scalar, e.action.target);
                }
                // D2-05905
                ServerMessage::MotionInterpChange(e) => {
                    let record = self
                        .world
                        .records
                        .get::<auberean_assets::animation::MotionDesc>(e.action.0);
                    if let (Some(md), Some(b)) = (record, self.body_mut(id)) {
                        b.minterp
                            .set_description(e.action.0, std::sync::Arc::new(md));
                        b.set_movement_active(true);
                    }
                }
                ServerMessage::InitializeMissile(e) => self.recv_initialize_missile(id, &e.action),
                ServerMessage::PushToward(e) => self.recv_push_toward(id, &e.action),
                m => {
                    if let Some(b) = self.body_mut(id) {
                        recv_on_body(b, &m);
                    }
                }
            }
        }
        waiting
    }

    /// Object `id`'s body is brought up to what the server describes of it: its velocity, rate of
    /// turn and scales; where it looks; the behaviors it plays (those of the network's it plays that
    /// are not described stop, those described it does not play start); its sliders, those the
    /// network moves and those described, each going to its described value (0 when not described)
    /// over the time its described speed takes; and the effects it plays (those it plays stop, its
    /// look's own apart, and those described start).
    // D2-06366, D2-13868, D2-13869, D2-13870, D2-13871
    pub(crate) fn take_description(&mut self, client: &Client, id: u64) {
        use auberean_client_sdk::protocol::types::PhysicsDesc as Pd;
        let Some(c) = client.model.object(id).map(|o| o.physics.clone()) else {
            return;
        };
        if let Some(b) = self.body_mut(id) {
            auberean_physics::motion::Owner::set_velocity(b, crate::pv(c.velocity));
            b.set_misc_active(true);
            b.set_omega_velocity(crate::pv(c.omega));
            if c.bits & Pd::VELOCITY_SCALE != 0 {
                b.set_velocity_scale(c.velocity_scale);
            }
            if c.bits & Pd::JUMP_SCALE != 0 {
                b.set_jump_scale(c.jump_scale);
            }
            b.acceleration_field_velocity = crate::pv(c.external_acceleration);
            b.update_times = c.stamps;
        }
        if c.look_at != 0 {
            self.look_at(id, c.look_at);
        }
        if c.bits & (Pd::HEAD_X | Pd::HEAD_Z) != 0
            && !self.player.as_ref().is_some_and(|p| p.id == id)
        {
            self.looks.entry(id).or_default().dir = (c.head_x, c.head_z);
        }
        let anims = &self.anims;
        let Some(f) = crate::figure_in(
            &mut self.player,
            &mut self.others,
            &mut self.held,
            &mut self.temporaries,
            id,
        ) else {
            return;
        };
        let running = f.modes.network_params();
        for r in &running {
            if !c.behaviors.iter().any(|d| d.behavior == r.behavior) {
                f.stop_behavior(anims, r.behavior);
            }
        }
        let records = &self.world.records;
        let network = records
            .get::<auberean_assets::animation::CharacterTemplate>(
                records.data_id_by_enum(0x4000_0001, 0x4000_0008),
            )
            .map(|t| t.sliders.clone())
            .unwrap_or_default();
        let mut sliders: std::collections::BTreeSet<u32> = f
            .sliders
            .network_sliders(|s| network.iter().find(|(n, _)| *n == s).map(|(_, c)| *c))
            .into_iter()
            .collect();
        sliders.extend(c.sliders.iter().map(|s| s.slider));
        for slider in sliders {
            let (value, speed) = c
                .sliders
                .iter()
                .find(|s| s.slider == slider)
                .map_or((0.0, 0.0), |s| (s.value, s.time));
            let (desired, moving) = (f.sliders.desired(slider), f.sliders.velocity(slider));
            #[allow(clippy::float_cmp)]
            if desired != value || moving != speed {
                let time = if speed.abs() >= 0.0002 {
                    ((desired - value) / speed).abs()
                } else {
                    0.0
                };
                f.sliders.do_slider(0, slider, value, time);
            }
        }
        for d in c
            .behaviors
            .iter()
            .filter(|d| !running.iter().any(|r| r.behavior == d.behavior))
        {
            self.play_behavior(id, &params_of_wire(d));
        }
        let playing = self
            .others
            .effects_mut(id)
            .map(|e| e.current_fx())
            .unwrap_or_default();
        for (fx, _) in playing {
            self.stop_fx(id, fx);
        }
        for fx in &c.fx {
            self.do_fx(id, fx.fx, fx.scalar);
        }
        if c.bits & (Pd::TARGET | Pd::TARGET_POSITION) != 0 {
            let target_position = crate::position_of_placement(&c.target_position);
            let target = (c.bits & Pd::TARGET != 0).then(|| {
                (
                    c.target,
                    c.bits & Pd::TARGET_SCALED != 0,
                    crate::pv(c.target_offset),
                )
            });
            let (land, table) = (&self.land, &mut self.table);
            let body = if self.player.as_ref().is_some_and(|p| p.id == id) {
                self.player.as_mut().map(|p| &mut p.body)
            } else {
                self.others.body_mut(id)
            };
            if let Some(b) = body {
                b.take_missile_description(
                    land,
                    table,
                    &mut auberean_physics::body::missile::NoObjects,
                    target_position,
                    c.bits & Pd::MISSILE_ACTIVATED != 0,
                    c.bits & Pd::MISSILE_MOVING != 0,
                    target,
                    crate::pv(c.velocity),
                );
            }
        }
    }

    /// The server moves a slider of object `object`: the player's own body keeps the sliders the
    /// character template leaves to the client. The body's animation wakes.
    // D2-05852, D2-15073, D2-05885
    fn recv_do_slider(&mut self, object: u64, s: &auberean_client_sdk::protocol::physics::Slide) {
        let is_player = self.player.as_ref().is_some_and(|p| p.id == object);
        if is_player && self.slider_is_the_clients(s.slider) {
            return;
        }
        if let Some(b) = self.body_mut(object) {
            b.set_animation_active(true);
        }
        if let Some(f) = crate::figure_in(
            &mut self.player,
            &mut self.others,
            &mut self.held,
            &mut self.temporaries,
            object,
        ) {
            f.sliders.do_slider(0, s.slider, s.value, s.time);
        }
    }

    /// Whether the game's character template names `slider` as one the network does not move.
    fn slider_is_the_clients(&self, slider: u32) -> bool {
        const TEMPLATE: (u32, u32) = (0x4000_0001, 0x4000_0008);
        let records = &self.world.records;
        let id = records.data_id_by_enum(TEMPLATE.0, TEMPLATE.1);
        records
            .get::<auberean_assets::animation::CharacterTemplate>(id)
            .and_then(|t| {
                t.sliders
                    .iter()
                    .find(|(s, _)| *s == slider)
                    .map(|(_, c)| *c)
            })
            .is_some_and(|controller| controller == 0)
    }

    /// The server pushes object `object` away from another: measured from where that object stands
    /// now; what the body kept of that object for a move stays as it was. None when that object is
    /// not here.
    // D2-05844
    fn recv_push_toward(
        &mut self,
        object: u64,
        a: &auberean_client_sdk::protocol::physics::TargetedImpulse,
    ) {
        let Some(at) = self.body(a.target).map(|t| t.position) else {
            return;
        };
        let Some(b) = self.body_mut(object) else {
            return;
        };
        let kept = b.targets.insert(
            a.target,
            auberean_physics::motion::moveto::TargetBody {
                position: at,
                radius: 0.0,
                height: 0.0,
            },
        );
        b.add_targeted_impulse(a.target, a.magnitude, a.falloff != 0, a.vertical);
        match kept {
            Some(k) => {
                b.targets.insert(a.target, k);
            }
            None => {
                b.targets.remove(&a.target);
            }
        }
    }

    /// The server launches object `object` as a missile: it flies at the velocity given toward
    /// where its target is.
    fn recv_initialize_missile(
        &mut self,
        object: u64,
        m: &auberean_client_sdk::protocol::physics::Missile,
    ) {
        let is_player = self.player.as_ref().is_some_and(|p| p.id == object);
        let body = if is_player {
            self.player.as_mut().map(|p| &mut p.body)
        } else {
            self.others.body_mut(object)
        };
        let Some(b) = body else {
            return;
        };
        let target = auberean_physics::target::Aim {
            object: m.target,
            position: crate::position_of_placement(&m.target_offset),
            offset: crate::pv(m.target_velocity),
            valid: m.initialized != 0,
        };
        b.receive_initialize_missile(
            &self.land,
            &mut self.table,
            &mut auberean_physics::body::missile::NoObjects,
            crate::pv(m.velocity),
            &target,
        );
    }
}
