//! The animation-hook drain, and what it counts.
//!
//! The drain reads every object's driver queue, turns
//! the physics-affecting hooks back into `dereth_physics` calls against the body's world, and
//! stashes the sound ones for the scene to play.
//!
//! **Why it is not in `dereth-animation`.** It needs [`crate::character::Character`] — the physics world
//! lives inside the local body — and `dereth-animation` is below this crate. The half that names only
//! `dereth_physics` types lives in that crate: `report_attacks` is
//! `dereth_physics::step::report_attacks`, and it returns the hit count instead of writing
//! `AttackStats::objects_hit`.
//!
//! **The two things the drain needs that are not simulation** — where a sound raised by an
//! object is heard, and which object sound table a `SoundType` hook resolves against — arrive through
//! `HookObject`, because both are fields of the scene's own object and neither is read by the
//! step. The sink is a trait for the same reason: `dereth_client_runtime::audio::SoundTrigger` is
//! `dereth-client`'s, so this crate names the two pushes rather than
//! the enum.

use std::collections::BTreeMap;

use dereth_primitives::{DataId, ObjectId, Vec3};

use crate::character::Character;
use crate::object_step::AsObjectSim;

/// A scene object, as the hook drain reads it: the simulation half plus the two presentation
/// facts a sound hook needs.
pub trait HookObject: AsObjectSim {
    /// Where a sound this object raises is heard: internal sound playback takes
    /// `&obj->position`, so it is the emitting object's own origin in the space the scene's
    /// listener works in.
    fn sound_origin(&self) -> Vec3;
    /// The object sound table a `SoundTable` hook resolves its
    /// `SoundType` against.
    fn sound_table(&self) -> Option<DataId>;
}

/// Where the drain puts the sound hooks it cannot play itself.
///
/// The client's implementor pushes `dereth_client_runtime::audio::SoundTrigger::Wave` and `::Table`; a
/// headless consumer can count them or drop them.
pub trait HookSounds {
    /// `SoundHook` / `SoundTweakedHook` — a direct wave id.
    fn wave(&mut self, id: DataId, at: Vec3, volume: f32, priority: f32, probability: f32);
    /// `SoundTableHook` — the object's own sound table, by `SoundType`.
    fn table(&mut self, table: DataId, stype: u32, at: Vec3);
}

/// What the `AttackHook`s did, cumulatively.
///
/// Five fields, for the same reason [`EtherealStats`] has five: a cone that was swung, a cone
/// that found nothing, a cone that found something, and a hit whose attacker had no
/// `SCRIPTED_COLLISION_PS` are different facts, and only the sum of them says the receiver ran.
/// `Attack` is **716 of 2,066** retail animations (1,053 instances), so unlike
/// `EtherealStats::nodraw_hooks` these are expected to move constantly in play.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttackStats {
    /// `AnimEvent::Attack` events drained, whatever became of them.
    pub hooks: u64,
    /// Of those, the ones whose object had no physics body in this build, so
    /// the physics attack operation was never reached. **Not** a success.
    pub no_body: u64,
    /// Distinct objects reported, summed over every cone —
    /// i.e. `AtkCollisionProfile`s handed to `DoCollision`. Retail calls the slot once per
    /// entry in the attack's object list, which is deduped by object id.
    pub objects_hit: u64,
    /// Hits whose **attacker** carried `SCRIPTED_COLLISION_PS` and whose
    /// default-script playback answered **1**. Same caveat as
    /// `SceneStats::scripts_played`: queued scripts, not emitters.
    pub scripts_played: u64,
    /// Hits whose attacker carried `SCRIPTED_COLLISION_PS` and whose `play_default_script`
    /// answered **0** — no `PhysicsScriptTable`, which takes the client's early branch.
    ///
    /// A hit whose attacker does *not* carry the bit is counted in neither: the client's bit
    /// test means the whole receiver does nothing, which is the ordinary case for a
    /// player swinging a sword. `objects_hit` is the denominator that says so.
    pub scripts_unplayed: u64,
}

/// What the ethereal hooks did, cumulatively.
///
/// Four fields rather than one distinguish outcomes that a single success counter would hide: a
/// hook that fired and took, a hook that fired and was **deferred** because a body was
/// standing in the object, and a hook that fired for an object with **no physics body at
/// all** are three different facts, and only the first is success. Without the third the
/// wiring could be entirely absent and every counter would still read zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EtherealStats {
    /// `AnimEvent::SetEthereal` events drained, whatever became of them.
    pub hooks: u64,
    /// Of those, asking for ethereal **on** — always unconditional in the client.
    pub hooks_on: u64,
    /// [`dereth_physics::EtherealResult::Applied`].
    pub applied: u64,
    /// [`dereth_physics::EtherealResult::Deferred`] — refused and queued for retry.
    pub deferred: u64,
    /// [`dereth_physics::EtherealResult::NoObject`]: the animation played on an object that
    /// has no physics body in this build — held, unplaced, or the hook arriving in the same
    /// frame its body was destroyed. **Not** a success, and not silently a success.
    pub no_body: u64,
    /// Hooks affecting the `NODRAW_PS` state. Retail carries **zero**
    /// `NoDrawHook`s in any of its 2,066 animations, so this is expected to stay 0 in play;
    /// it is counted rather than assumed so that "never fires" is a measurement.
    pub nodraw_hooks: u64,
    /// Hooks affecting angular velocity. The retail dat carries `SET_OMEGA` in **8 of its 2,066
    /// animations** — 0.39% — so this is a small arm and is counted rather than assumed for
    /// the same reason `nodraw_hooks` is: "wired and never fires" and "not wired" are
    /// indistinguishable without a number.
    pub omega_hooks: u64,
    /// Of those, the ones whose object had **no physics body** in this build, so the omega
    /// was raised and landed nowhere. The third state, exactly as `no_body` is for
    /// ethereal: without it, a build where every omega hook was lost reads identically to one
    /// where every omega hook took.
    pub omega_hooks_no_body: u64,
    /// The scale hook's object-scale half runs once per
    /// [`dereth_animation::AnimEvent::SetScale`], which the driver raises once for an immediate
    /// `SetScale` and once per step for a ramp. 122 shipped `SCALE` hooks,
    /// all of them in physics scripts and all of them immediate.
    pub scale_hooks: u64,
    /// Of those, the ones whose object had no physics body, so the collision half was lost
    /// while the part array still shrank. The third state again.
    pub scale_hooks_no_body: u64,
    /// The particle-emitter hook calls the object's particle-emitter handler. Both halves are
    /// resolved inside [`dereth_animation`] — the immediate one by immediate script playback, while
    /// delayed playback uses a timed hook on the object's own list — so this arm has no call to
    /// make and exists to count, the way `PhysicsNotice::ObjectCollision`'s empty arm does: a
    /// wildcard would not distinguish "arrives and is handled elsewhere" from "arrives and is
    /// dropped".
    pub pes_hooks: u64,
    /// Of those, the ones that armed a timer rather than playing at once — the 23% of shipped
    /// `CALL_PES` hooks that carry a pause.
    pub pes_hooks_delayed: u64,
}

/// The object plays its default script at the tail of collision handling, which is the
/// client's whole impact effect.
///
/// With no cell, default-script playback returns success without playing. With no physics
/// script table, it returns failure. Otherwise it resolves the default script type through
/// that table at the default intensity and queues the resulting script.
///
/// This is `play_script(default_script, default_script_intensity)`, exactly
/// what [`dereth_animation::MotionDriver::play_default_script`] already was. Same routing rule as
/// `play_script_type` and the same return: `true` means the script was queued.
pub fn play_default_script<S: AsObjectSim>(
    objects: &BTreeMap<ObjectId, S>,
    character: &Option<Character>,
    player_object: Option<ObjectId>,
    id: ObjectId,
) -> bool {
    if player_object == Some(id) {
        if let Some(c) = character.as_ref() {
            return c.driver_mut().play_default_script(None);
        }
    }
    match objects.get(&id) {
        Some(o) => o.sim().driver.borrow_mut().play_default_script(None),
        None => false,
    }
}

/// Run this step's animation hooks.
///
/// The client executes the deferred hook queue at the end of the physics
/// step and each hook calls straight into the physics body; here the hooks arrive as
/// [`dereth_animation::AnimEvent`]s and this is the one place they are turned back into calls.
///
/// Three are wired:
///
/// * the angular-velocity hook sets the object's omega, with the axis
///   verbatim into `omega_vector`, which the integrator already turns the frame by.
///   The retail dat puts `SET_OMEGA` on **8 of 2,066** animations (0.39%), so this is a
///   narrow arm and is not sold as more. The setter must not activate the object; retail's
///   does not.
/// * the ethereal hook updates the physics world's ethereal state,
///   passing the hook's own `0` for the client's unused second argument. **This is the
///   whole reason a door you have opened stops blocking you**, and the reverse: nothing
///   else in the client moves `ETHEREAL_PS`.
/// * the no-draw hook updates `NODRAW_PS`. The part-array half is already
///   done inside [`dereth_animation`]'s driver; the state
///   bit was the missing half. Measured over the retail dat, **no animation carries a
///   `NoDrawHook`** — 0 of 2,066 — so this arm is expected never to fire in play and is
///   counted so that the expectation is checkable rather than assumed.
///
/// Sound is *stashed* rather than played: `take_sound_events` collects it.
/// Everything else is dropped.
///
/// The physics world is [`crate::character::Character`]'s, so an object with no body
/// yet — held, unplaced, or created this frame — counts as
/// [`EtherealStats::no_body`] and the hook is lost, which is what the client does with an
/// animation on an object it has not put in a cell.
///
/// **The attack hook is the largest hook type in the dat:** it enters the physics world's attack
/// path for **716 of the 2,066 animations** and 1,053 instances, against `EtherealHook`'s 47 and
/// `NoDrawHook`'s zero. See `report_attacks` for the half that is not physics.
pub fn process_hooks<S: HookObject, K: HookSounds>(
    objects: &mut BTreeMap<ObjectId, S>,
    character: &mut Option<Character>,
    sounds: &mut K,
    character_sound_table: Option<DataId>,
    player_object: Option<ObjectId>,
    ethereal: &mut EtherealStats,
    attack: &mut AttackStats,
) {
    let drained = execute_anim_hooks(
        objects,
        character,
        sounds,
        character_sound_table,
        ethereal,
        attack,
    );
    for (is_body, attacker, plays) in drained {
        if !plays {
            continue;
        }
        // **Routed by handle, not by id, and that is not a shortcut.** The client's hook first
        // reads its own physics object, so retail reaches the
        // attacker's own physics body with no table lookup of any kind.
        // `play_default_script`'s lookup is keyed on `player_object`, which
        // is **`None` until `ObjectStream::player` names the body**, so asking it about the
        // body's own id answers `false` and plays nothing (`scripts_unplayed: 1` against working
        // wiring). The same hazard is live on the collision path, which calls
        // `play_default_script(body_id)`.
        let played = if is_body {
            character
                .as_ref()
                .is_some_and(|c| c.driver_mut().play_default_script(None))
        } else {
            play_default_script(objects, character, player_object, attacker)
        };
        if played {
            attack.scripts_played += 1;
        } else {
            attack.scripts_unplayed += 1;
        }
    }
}

/// The drain itself. Returns `report_attacks`'s triples, because `play_default_script` needs
/// the whole of `self` and this borrows it apart.
fn execute_anim_hooks<S: HookObject, K: HookSounds>(
    objects: &mut BTreeMap<ObjectId, S>,
    character: &mut Option<Character>,
    sounds: &mut K,
    character_sound_table: Option<DataId>,
    ethereal: &mut EtherealStats,
    attack: &mut AttackStats,
) -> Vec<(bool, ObjectId, bool)> {
    // The physics-affecting half, held until every driver's queue has been drained: the
    // drain borrows the object table and applying borrows the physics world, which lives
    // inside the character.
    let mut deferred: Vec<(
        Option<dereth_physics::PhysHandle>,
        dereth_animation::AnimEvent,
    )> = Vec::new();
    let mut attacks: Vec<(bool, ObjectId, bool)> = Vec::new();
    let Some(c) = character.as_mut() else {
        // No body means no physics world at all — the free-camera build. The queues are
        // still drained, because an undrained queue grows without bound, and every
        // ethereal hook is counted as having reached no body, which is exactly true.
        for o in objects.values_mut() {
            let at = o.sound_origin();
            let table = o.sound_table();
            for e in o.sim_mut().driver.borrow_mut().take_events() {
                match e {
                    dereth_animation::AnimEvent::PlaySound {
                        gid,
                        volume,
                        priority,
                        probability,
                    } => {
                        sounds.wave(gid, at, volume, priority, probability);
                    }
                    dereth_animation::AnimEvent::PlaySoundType { kind } => {
                        if let Some(table) = table {
                            sounds.table(table, kind.0, at);
                        }
                    }
                    dereth_animation::AnimEvent::SetEthereal(on) => {
                        ethereal.hooks += 1;
                        ethereal.hooks_on += u64::from(on);
                        ethereal.no_body += 1;
                    }
                    dereth_animation::AnimEvent::SetNoDraw(_) => {
                        ethereal.nodraw_hooks += 1;
                    }
                    // The free-camera build has no physics world at all, so
                    // every omega hook reaches no body — counted as exactly that rather
                    // than silently dropped.
                    dereth_animation::AnimEvent::SetOmega(_) => {
                        ethereal.omega_hooks += 1;
                        ethereal.omega_hooks_no_body += 1;
                    }
                    dereth_animation::AnimEvent::Attack { .. } => {
                        attack.hooks += 1;
                        attack.no_body += 1;
                    }
                    // The free-camera halves of the two arms
                    // below. No physics world, so no `scale` to write; the `CallPes`
                    // timer is the driver's own and does not need one.
                    dereth_animation::AnimEvent::SetScale(_) => {
                        ethereal.scale_hooks += 1;
                        ethereal.scale_hooks_no_body += 1;
                    }
                    dereth_animation::AnimEvent::CallPes { pause, .. } => {
                        ethereal.pes_hooks += 1;
                        ethereal.pes_hooks_delayed +=
                            u64::from(pause >= dereth_primitives::num::consts::EPSILON);
                    }
                    _ => {}
                }
            }
        }
        return attacks;
    };
    for (id, o) in objects.iter_mut() {
        let at = o.sound_origin();
        let table = o.sound_table();
        let h = c.world.by_object_id(*id);
        for e in o.sim_mut().driver.borrow_mut().take_events() {
            match e {
                dereth_animation::AnimEvent::PlaySound {
                    gid,
                    volume,
                    priority,
                    probability,
                } => {
                    sounds.wave(gid, at, volume, priority, probability);
                }
                dereth_animation::AnimEvent::PlaySoundType { kind } => {
                    if let Some(table) = table {
                        sounds.table(table, kind.0, at);
                    } else {
                        tracing::debug!(
                            "sound type {} on {id:?}, which has no sound table",
                            kind.0
                        );
                    }
                }
                other => deferred.push((h, other)),
            }
        }
    }
    {
        let at = c.render_frame().origin;
        let table = character_sound_table;
        let h = Some(c.handle);
        for e in c.take_anim_events() {
            match e {
                dereth_animation::AnimEvent::PlaySound {
                    gid,
                    volume,
                    priority,
                    probability,
                } => {
                    sounds.wave(gid, at, volume, priority, probability);
                }
                dereth_animation::AnimEvent::PlaySoundType { kind } => {
                    if let Some(table) = table {
                        sounds.table(table, kind.0, at);
                    }
                }
                other => deferred.push((h, other)),
            }
        }
    }
    for (h, e) in deferred {
        match e {
            dereth_animation::AnimEvent::SetEthereal(on) => {
                ethereal.hooks += 1;
                ethereal.hooks_on += u64::from(on);
                let Some(h) = h else {
                    ethereal.no_body += 1;
                    continue;
                };
                // `set_ethereal(object, ethereal, 0)`: the hook's own literal
                // zero, which the client's body never reads.
                match c.world.set_ethereal(h, on, false) {
                    dereth_physics::EtherealResult::Applied => ethereal.applied += 1,
                    dereth_physics::EtherealResult::Deferred => ethereal.deferred += 1,
                    dereth_physics::EtherealResult::NoObject => ethereal.no_body += 1,
                }
            }
            dereth_animation::AnimEvent::SetNoDraw(on) => {
                ethereal.nodraw_hooks += 1;
                if let Some(o) = h.and_then(|h| c.world.get_mut(h)) {
                    o.state.set_nodraw_bit(on);
                }
            }
            // The angular-velocity hook passes its axis directly
            // to the body's setter with notification enabled. That setter only assigns
            // `omega_vector`: it does not add to it or activate the object. The last hook
            // of a frame therefore wins, with no activation of its own.
            //
            // The receiver was already waiting: `update_physics_internal` ends with
            // `grotate(frame, omega_vector * quantum)`, so an object that is awake turns.
            // An object that is **asleep** does not, and that is retail; the physics crate's
            // set-omega test is the fixture for it.
            dereth_animation::AnimEvent::SetOmega(w) => {
                ethereal.omega_hooks += 1;
                if let Some(o) = h.and_then(|h| c.world.get_mut(h)) {
                    o.set_omega(w);
                } else {
                    ethereal.omega_hooks_no_body += 1;
                }
            }
            // The attack hook passes this cone to physics; it is retail's only attack caller.
            dereth_animation::AnimEvent::Attack { cone } => {
                attack.hooks += 1;
                let Some(h) = h else {
                    attack.no_body += 1;
                    continue;
                };
                // `dereth_animation`'s `part_index` is the dat's `u32`; `AttackCone::part_index` in
                // the client is the signed `long` copied into the attack info, and 681
                // of the 1,053 shipped cones carry `0xFFFFFFFF`. A reinterpret, not a
                // conversion — `try_from` would refuse exactly those 681.
                #[allow(clippy::cast_possible_wrap)]
                let cone = dereth_physics::detect::AttackCone {
                    part_index: cone.part_index as i32,
                    left: cone.left,
                    right: cone.right,
                    radius: cone.radius,
                    height: cone.height,
                };
                let hits = c.world.attack(h, &cone);
                let body = c.handle;
                attack.objects_hit +=
                    dereth_physics::step::report_attacks(&c.world, h, body, &hits, &mut attacks);
            }
            // The scale hook carries `(end, time)`, and its immediate arm is two
            // stores: the interpolated scale target, which [`dereth_animation`] already handled,
            // and `scale`. That second store is this
            // arm. It is not cosmetic: `transition`, `check_collision` and `SetPosition`
            // each pass `scale` to collision as the scale its spheres are swept at, and
            // `attack` sizes its cone with it. Without it a script that doubles a creature
            // leaves its collision sphere the old size, and a script that halves one leaves
            // the player unable to walk where the thing visibly is not.
            dereth_animation::AnimEvent::SetScale(s) => {
                ethereal.scale_hooks += 1;
                if let Some(o) = h.and_then(|h| c.world.get_mut(h)) {
                    o.scale = s;
                } else {
                    ethereal.scale_hooks_no_body += 1;
                }
            }
            // Both arms of `CallPES` are resolved inside the
            // driver — the immediate one during script playback, the delayed one by a
            // timed hook on the object's own list that the per-frame hook update steps, so there is
            // nothing for this drain to do and this arm counts rather than calls. Written out
            // instead of left to the wildcard for the reason `character.rs`'s
            // `PhysicsNotice::ObjectCollision` arm is: an explicit empty arm is evidence
            // that the event was read, and a wildcard is evidence of nothing.
            dereth_animation::AnimEvent::CallPes { pause, .. } => {
                ethereal.pes_hooks += 1;
                ethereal.pes_hooks_delayed +=
                    u64::from(pause >= dereth_primitives::num::consts::EPSILON);
            }
            _ => {}
        }
    }
    attacks
}
