//! The behaviors a body plays in one element and mode, and which behavior holds each layer.
//!
//! Asking for a behavior looks it up in the character template (is it an action, a hold?) and
//! the behavior table (which animation key and layer in this element and mode, how long it ramps
//! in, which effect starts it), and asks the object how fast its animation cycles. The new track
//! takes its layer from whatever held it, which is stopped. Behaviors the server asked for with
//! someone waiting on them count as the server's own: while any plays, the idle behaviors are
//! stopped, and when the last ends they start again.
//!
//! Each frame every track advances, gathers its effect script's triggers, tells whoever waits on
//! it when a hold has repeated enough or an action begins ramping down early, hands its layer on
//! when it starts ramping down, and is dropped once its weight is spent.

use auberean_assets::animation::{BehaviorKey, BehaviorTable};

use crate::host::{error, Event, Host};
use crate::queries::{behavior_flags, TableQueries};
use crate::table::Table;
use crate::track::{Params, Track};

/// The behavior a body shows while it moves.
pub const MOVE: u32 = 0x4000_0001;
/// The idle behaviors.
pub const IDLE: u32 = 0x4000_0002;
pub const IDLE_FIDGET: u32 = 0x4000_0003;

/// The behaviors of one element and mode.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Behaviors {
    pub element: u32,
    pub mode: u32,
    /// How many behaviors the server asked for (with someone waiting) are playing.
    pub server_playing: u32,
    /// The idles are to start again.
    pub start_idle: bool,
    /// The idles are to stop.
    pub stop_idle: bool,
    /// The moving behavior plays: its animations add into the pose by their own bone weights.
    pub playing_move: bool,
    pub tracks: Table<Track>,
    /// The behavior holding each layer.
    pub active_by_layer: Table<u32>,
    /// The behavior that last let each layer go.
    pub inactive_by_layer: Table<u32>,
}

/// Tell whoever waits on a track that it is over, once.
// D2-14915
pub fn send_and_clear_callback(t: &mut Track, err: u32, host: &mut dyn Host) {
    let context = t.params.context;
    if context == 0 {
        return;
    }
    host.event(if t.callback_with_release {
        Event::ReleaseDone {
            context,
            error: err,
        }
    } else {
        Event::BehaviorDone {
            context,
            error: err,
        }
    });
    t.params.context = 0;
}

/// What a behavior's flags ask of the object when it starts: children faded out, its effect,
/// and for the player's own body the action lock and the camera.
// D2-14916
pub fn start_flags(t: &Track, host: &mut dyn Host) {
    let p = &t.params;
    if p.fade_children {
        host.event(Event::ChildrenOpacity {
            opacity: 0.0,
            time: t.ramp_time,
        });
    }
    if p.fx != 0 {
        host.event(Event::DoFx {
            fx: p.fx,
            intensity: 1.0,
        });
    }
    if !host.is_the_player() {
        return;
    }
    if p.lock_actions {
        host.event(Event::LockInput);
    }
    let time = if p.camera_hold {
        -1.0
    } else {
        1.0 / t.cycle_velocity
    };
    if p.camera_parent {
        host.event(Event::CameraAnimParent {
            time,
            ramp: t.ramp_time,
        });
    }
    if p.camera_targets {
        host.event(Event::CameraAnimTarget {
            time,
            ramp: t.ramp_time,
        });
    }
    if p.camera_restore {
        host.event(Event::CameraBack);
    }
    if p.camera_target != 0 {
        host.event(Event::TargetedCamera {
            target: p.camera_target,
            behavior: p.camera_behavior,
            words: (0x4000_0002, 0x4000_0001),
            time: 1.0 / t.cycle_velocity,
            ramp: 0.5,
        });
    }
}

impl Behaviors {
    /// An empty tracker.
    // D2-14948
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What a behavior's flags ask of the object as it ramps down: children faded back in, the
    /// object destroyed, the effect target let go, the camera restored, its effect stopped; a
    /// server behavior ending counts down, and the idles start again when none is left.
    // D2-14917
    fn ramp_down_flags(&mut self, behavior: u32, host: &mut dyn Host) {
        let Some(t) = self.tracks.get(behavior) else {
            return;
        };
        let p = &t.params;
        if p.fade_children {
            host.event(Event::ChildrenOpacity {
                opacity: 1.0,
                time: t.ramp_time,
            });
        }
        if p.destroy_on_completion {
            host.event(Event::Remove);
        }
        if p.target != 0 || p.clone_visual != 0 {
            host.event(Event::ClearFxTarget);
        }
        if host.is_the_player() && (p.camera_parent || p.camera_targets) {
            host.event(Event::CameraBack);
        }
        if p.fx != 0 {
            host.event(Event::StopFx { fx: p.fx });
        }
        if (p.context != 0 || t.is_hold || t.was_hold) && !p.leave_idle_alone {
            self.server_playing = self.server_playing.wrapping_sub(1);
        }
        if self.server_playing == 0 {
            self.start_idle = true;
        }
    }

    /// Gather every track's triggers into the object's trigger update.
    // D2-14925
    pub fn add_triggers(&mut self, host: &mut dyn Host) {
        let keys: Vec<u32> = self.tracks.keys().collect();
        for b in keys {
            let t = self.tracks.get_mut(b).expect("listed");
            let ramping = t.is_ramping_down();
            let (mut gathered, mut looped) = (std::mem::take(&mut t.gathered), t.looped);
            let (dice, triggers) = split(host);
            if let Some(u) = triggers {
                u.add_track_triggers(&mut gathered, ramping, &mut looped, dice);
            }
            let t = self.tracks.get_mut(b).expect("listed");
            t.gathered = gathered;
            t.looped = looped;
        }
    }

    /// Whether any playing behavior moves the first-person camera.
    // D2-14926
    #[must_use]
    pub fn any_dynamic_fp_camera(&self) -> bool {
        self.tracks.iter().any(|(_, t)| t.dynamic_fp_camera)
    }

    /// Let go of a hold: it finishes as an action. `false` when the behavior is not a held one.
    // D2-14931
    pub fn release_behavior(&mut self, behavior: u32) -> bool {
        self.tracks
            .get_mut(behavior)
            .is_some_and(|t| t.is_hold && t.release())
    }

    /// A behavior's track.
    // D2-14932
    #[must_use]
    pub fn track(&self, behavior: u32) -> Option<&Track> {
        self.tracks.get(behavior)
    }

    pub fn track_mut(&mut self, behavior: u32) -> Option<&mut Track> {
        self.tracks.get_mut(behavior)
    }

    /// Whether a behavior plays.
    // D2-14933
    #[must_use]
    pub fn is_doing_behavior(&self, behavior: u32) -> bool {
        self.tracks.contains(behavior)
    }

    fn key(&self, behavior: u32) -> BehaviorKey {
        BehaviorKey {
            element: self.element,
            mode: self.mode,
            behavior,
        }
    }

    /// Advance every behavior `dt` seconds; see the module's description. Dropped tracks stop the
    /// effects they started and give back their layer; the idles start again when no server
    /// behavior plays and the first idle is not playing.
    // D2-14938
    pub fn update(&mut self, table: &BehaviorTable, dt: f32, host: &mut dyn Host) {
        self.playing_move = false;
        let mut over: Vec<u32> = Vec::new();
        let keys: Vec<u32> = self.tracks.keys().collect();
        for b in keys {
            if b == MOVE {
                self.playing_move = true;
            }
            let key = self.key(b);
            let cv = host.combo_cycle_velocity(&key);
            let t = self.tracks.get_mut(b).expect("listed");
            t.cycle_velocity = cv;
            t.update(dt);
            if t.params.fx_script != 0 {
                let (script, time) = (t.params.fx_script, t.time);
                let mut g = std::mem::take(&mut t.gathered);
                host.script_triggers(script, time, &mut g);
                self.tracks.get_mut(b).expect("listed").gathered = g;
            }
            let t = self.tracks.get_mut(b).expect("listed");
            if t.is_hold && t.repeats >= t.params.hold_cycles && t.params.context != 0 {
                send_and_clear_callback(t, error::NONE, host);
            }
            if t.ramping_down {
                if t.params.context != 0 && t.params.early_callback {
                    send_and_clear_callback(t, error::NONE, host);
                }
                if t.started_ramping_down {
                    self.ramp_down_flags(b, host);
                    let t = self.tracks.get_mut(b).expect("listed");
                    t.started_ramping_down = false;
                    let combo = table.combo(&key);
                    if combo.is_valid() {
                        if self.active_by_layer.get(combo.layer) == Some(&b) {
                            self.active_by_layer.remove(combo.layer);
                            self.inactive_by_layer.set(combo.layer, b);
                        }
                    } else {
                        t.weight = -1.0;
                    }
                }
            }
            let t = self.tracks.get_mut(b).expect("listed");
            if t.weight <= 0.0 {
                if t.params.lock_actions {
                    host.event(Event::UnlockInput);
                }
                over.push(t.params.behavior);
                t.weight = -1.0;
                t.weight_velocity = 0.0;
                let (mut g, mut looped, ramping) = (
                    std::mem::take(&mut t.gathered),
                    t.looped,
                    t.is_ramping_down(),
                );
                let (dice, triggers) = split(host);
                let had = triggers.is_some();
                if let Some(u) = triggers {
                    u.add_track_triggers(&mut g, ramping, &mut looped, dice);
                    u.on_track_terminated(&g, &mut looped);
                }
                let t = self.tracks.get_mut(b).expect("listed");
                t.gathered = g;
                t.looped = looped;
                if had {
                    t.terminated_fx = true;
                }
                let combo = table.combo(&key);
                if combo.is_valid() {
                    self.inactive_by_layer.remove(combo.layer);
                    let t = self.tracks.get_mut(b).expect("listed");
                    send_and_clear_callback(t, error::NONE, host);
                }
            }
        }
        for b in over {
            self.tracks.remove(b);
        }
        if self.server_playing == 0 && !self.tracks.contains(IDLE) {
            self.start_idle = true;
        }
    }

    /// Stop a behavior: it ramps out over its ramp time (at once without one), whoever waits on it
    /// is told it was stopped, and its layer is marked let go. Returns 0, or why the layer could
    /// not be handed back: another behavior holds it ([`error::LAYER_TAKEN`], [`error::LAYER_EMPTY`]),
    /// or the behavior does not play ([`error::NOT_PLAYING`]).
    // D2-14939
    pub fn stop_behavior(
        &mut self,
        table: &BehaviorTable,
        behavior: u32,
        host: &mut dyn Host,
    ) -> u32 {
        if !self.tracks.contains(behavior) {
            return error::NOT_PLAYING;
        }
        let key = self.key(behavior);
        let combo = table.combo(&key);
        let ramp = table.behavior_ramp_time(behavior);
        let t = self.tracks.get_mut(behavior).expect("present");
        if ramp == 0.0 {
            t.weight = -1.0;
            t.weight_velocity = 0.0;
        } else {
            t.weight_velocity = -1.0 / ramp;
        }
        self.ramp_down_flags(behavior, host);
        let t = self.tracks.get_mut(behavior).expect("present");
        if t.params.lock_actions {
            host.event(Event::UnlockInput);
        }
        t.stopped_action = true;
        t.ramping_down = true;
        send_and_clear_callback(t, error::STOPPED, host);
        let layer = combo.layer;
        match self.inactive_by_layer.get(layer).copied() {
            None => {
                if let Some(&holder) = self.active_by_layer.get(layer) {
                    if holder != behavior {
                        self.inactive_by_layer.set(layer, behavior);
                        return error::LAYER_EMPTY;
                    }
                }
                self.active_by_layer.remove(layer);
                self.inactive_by_layer.set(layer, behavior);
            }
            Some(b) if b != behavior => {
                self.inactive_by_layer.set(layer, behavior);
                return error::LAYER_TAKEN;
            }
            Some(_) => {}
        }
        0
    }

    /// Stop every behavior the server asked for with someone waiting.
    // D2-14940
    pub fn stop_all_server_behaviors(&mut self, table: &BehaviorTable, host: &mut dyn Host) {
        let waiting: Vec<u32> = self
            .tracks
            .iter()
            .filter(|(_, t)| t.params.context != 0)
            .map(|(_, t)| t.params.behavior)
            .collect();
        for b in waiting {
            self.stop_behavior(table, b, host);
        }
    }

    /// Stop the behavior holding a layer: [`error::LAYER_TAKEN`] when the layer was let go, else
    /// what stopping its holder returns ([`error::LAYER_EMPTY`] when none holds it).
    // D2-14941
    pub fn stop_layer(&mut self, table: &BehaviorTable, layer: u32, host: &mut dyn Host) -> u32 {
        if self.inactive_by_layer.contains(layer) {
            return error::LAYER_TAKEN;
        }
        match self.active_by_layer.get(layer).copied() {
            None => error::LAYER_EMPTY,
            Some(b) => self.stop_behavior(table, b, host),
        }
    }

    /// Stop every behavior a move cancels, the last-walked first.
    // D2-14945
    pub fn stop_move_to_cancels(&mut self, table: &BehaviorTable, host: &mut dyn Host) {
        let mut cancel: Vec<u32> = self
            .tracks
            .iter()
            .filter(|(_, t)| t.params.move_to_cancels)
            .map(|(b, _)| b)
            .collect();
        cancel.reverse();
        for b in cancel {
            self.stop_behavior(table, b, host);
        }
    }

    /// The parameters of every behavior to tell the network about: weighing something, not ramping
    /// down, and sending events.
    // D2-14957
    #[must_use]
    pub fn network_params(&self) -> Vec<Params> {
        self.tracks
            .iter()
            .filter(|(_, t)| t.weight > 0.0 && !t.ramping_down && t.params.send_event)
            .map(|(_, t)| t.params.clone())
            .collect()
    }

    /// The tracker of `element` and `mode`, carrying over the behaviors of `from` (a tracker being
    /// left) that have a combination there, and starting the table's initial behaviors at full
    /// weight. A carried behavior with no combination or no animation is ended as stopped (its
    /// ramping-down flags counted against this tracker, as the client does); one carried over takes
    /// its waiting context along, so the tracker left behind no longer answers for it.
    // D2-14962
    pub fn init(
        &mut self,
        table: &BehaviorTable,
        element: u32,
        mode: u32,
        from: Option<&mut Behaviors>,
        host: &mut dyn Host,
    ) {
        self.element = element;
        self.mode = mode;
        if let Some(from) = from {
            let keys: Vec<u32> = from.tracks.keys().collect();
            for fb in keys {
                let old = from.tracks.get_mut(fb).expect("listed");
                let mut t = old.clone();
                let b = t.params.behavior;
                let key = self.key(b);
                let combo = table.combo(&key);
                let cv = if combo.is_valid() {
                    host.combo_cycle_velocity(&key)
                } else {
                    0.0
                };
                if cv == 0.0 {
                    self.ramp_down_flags_of(old, host);
                    if old.params.lock_actions {
                        host.event(Event::UnlockInput);
                    }
                    send_and_clear_callback(old, error::STOPPED, host);
                    continue;
                }
                t.cycle_velocity = cv;
                old.params.context = 0;
                t.anim_key = combo.anim_key;
                t.layer = combo.layer;
                let ramping = t.ramping_down;
                let counts = t.params.context != 0 && !t.params.leave_idle_alone;
                self.tracks.set(b, t);
                if ramping {
                    self.inactive_by_layer.set(combo.layer, b);
                } else {
                    self.active_by_layer.set(combo.layer, b);
                    if counts {
                        self.server_playing = self.server_playing.wrapping_add(1);
                        if self.server_playing == 1 {
                            self.stop_idle = true;
                        }
                    }
                }
            }
        }
        for &b in table.init_behaviors() {
            let start = table.behavior_start(b);
            let flags = host.behavior_flags(b).unwrap_or(0);
            let key = self.key(b);
            let combo = table.combo(&key);
            if !combo.is_valid() {
                continue;
            }
            let cv = host.combo_cycle_velocity(&key);
            if cv == 0.0 {
                continue;
            }
            let mut t = Track {
                weight: 1.0,
                cycle_velocity: cv,
                is_action: flags & behavior_flags::ACTION != 0,
                is_hold: flags & behavior_flags::HOLD != 0,
                dynamic_fp_camera: flags & behavior_flags::DYNAMIC_FP_CAMERA != 0,
                layer: combo.layer,
                anim_key: combo.anim_key,
                ramp_time: start.ramp_time,
                ..Track::default()
            };
            t.params.behavior = b;
            t.params.context = 0;
            t.params.send_event = false;
            self.active_by_layer.set(combo.layer, b);
            self.tracks.set(b, t);
        }
    }

    /// The ramping-down flags of a track that is not (or no longer) in the tracker.
    fn ramp_down_flags_of(&mut self, t: &Track, host: &mut dyn Host) {
        let b = t.params.behavior;
        let kept = self.tracks.remove(b);
        self.tracks.add(b, t.clone());
        self.ramp_down_flags(b, host);
        self.tracks.remove(b);
        if let Some(k) = kept {
            self.tracks.add(b, k);
        }
    }

    /// Play a behavior.
    ///
    /// It fails (telling whoever waits why) when it already plays and is not ramping down, when
    /// the template does not know it, when this element and mode have no combination for it, or
    /// when no animation plays it. A behavior that is neither an action nor a hold tells whoever
    /// waits at once. Otherwise its starting effect fires; it takes its layer from the behavior that
    /// held it (stopped) and from one that let it go (ended at once); its weight starts at 0
    /// rising over its ramp time (at 1 without one); a hold without a loop plays as an action.
    // D2-14963, D2-06443, D2-06442
    pub fn do_behavior(
        &mut self,
        table: &BehaviorTable,
        params: &Params,
        host: &mut dyn Host,
    ) -> bool {
        let mut p = params.clone();
        let b = p.behavior;
        if self.tracks.get(b).is_some_and(|t| !t.ramping_down) {
            if p.context != 0 {
                host.event(Event::BehaviorDone {
                    context: p.context,
                    error: error::ALREADY_PLAYING,
                });
            }
            return false;
        }
        let Some(flags) = host.behavior_flags(b) else {
            if p.context != 0 {
                host.event(Event::BehaviorDone {
                    context: p.context,
                    error: error::UNKNOWN_BEHAVIOR,
                });
            }
            return false;
        };
        let is_action = flags & behavior_flags::ACTION != 0;
        let is_hold = flags & behavior_flags::HOLD != 0;
        if !is_action && !is_hold && p.context != 0 {
            host.event(Event::BehaviorDone {
                context: p.context,
                error: error::NONE,
            });
            p.context = 0;
        }
        let start = table.behavior_start(b);
        let key = self.key(b);
        let combo = table.combo(&key);
        if !combo.is_valid() {
            if p.context != 0 {
                host.event(Event::BehaviorDone {
                    context: p.context,
                    error: error::NO_COMBO,
                });
            }
            return false;
        }
        let cv = host.combo_cycle_velocity(&key);
        if cv == 0.0 {
            if p.context != 0 {
                host.event(Event::BehaviorDone {
                    context: p.context,
                    error: error::NO_ANIMATION,
                });
            }
            return false;
        }
        if let Some(gone) = self.inactive_by_layer.remove(combo.layer) {
            if let Some(mut t) = self.tracks.remove(gone) {
                send_and_clear_callback(&mut t, error::NONE, host);
                if !t.terminated_fx {
                    let mut looped = t.looped;
                    if let Some(u) = host.triggers() {
                        u.on_track_terminated(&t.gathered, &mut looped);
                    }
                }
            }
        }
        if start.effect != 0 {
            if let Some(u) = host.triggers() {
                u.once.add(
                    start.effect,
                    crate::triggers::trigger(start.effect, 0.0, 1.0, true),
                );
            }
        }
        let mut t = Track {
            params: p,
            cycle_velocity: cv,
            is_action,
            is_hold,
            dynamic_fp_camera: flags & behavior_flags::DYNAMIC_FP_CAMERA != 0,
            ramp_time: start.ramp_time,
            layer: combo.layer,
            anim_key: combo.anim_key,
            ..Track::default()
        };
        if start.ramp_time == 0.0 {
            t.weight = 1.0;
        } else {
            t.weight = 0.0;
            t.weight_velocity = 1.0 / start.ramp_time;
        }
        start_flags(&t, host);
        if t.is_hold {
            match host.hold_loop(&key) {
                Some((s, e)) if s != 0.0 || e != 0.0 => {
                    t.loop_start = s;
                    t.loop_end = e;
                }
                _ => {
                    t.is_hold = false;
                    t.is_action = true;
                }
            }
        }
        if t.params.context != 0 && !t.params.leave_idle_alone {
            self.server_playing = self.server_playing.wrapping_add(1);
            if self.server_playing == 1 {
                self.stop_idle = true;
            }
        }
        if let Some(&holder) = self.active_by_layer.get(combo.layer) {
            if holder != b {
                self.stop_behavior(table, holder, host);
            }
        }
        self.active_by_layer.set(combo.layer, b);
        self.tracks.set(b, t);
        true
    }

    /// Play a behavior nobody waits on.
    // D2-14964
    pub fn do_autonomous_behavior(
        &mut self,
        table: &BehaviorTable,
        behavior: u32,
        host: &mut dyn Host,
    ) -> bool {
        let p = Params {
            context: 0,
            ..Params::behavior(behavior)
        };
        self.do_behavior(table, &p, host)
    }

    /// Discard the tracker: destroy it (see [`Behaviors::destroy`]) and let it go.
    // D2-14937
    pub fn discard(mut self, host: &mut dyn Host) {
        self.destroy(host);
    }

    /// The tracker is being discarded: every track's weight is spent and its triggers gathered
    /// (so the effects it started stop), and faded children come back.
    // D2-14924
    pub fn destroy(&mut self, host: &mut dyn Host) {
        let keys: Vec<u32> = self.tracks.keys().collect();
        for b in keys {
            let t = self.tracks.get_mut(b).expect("listed");
            t.weight = -1.0;
            let (mut g, mut looped, ramping) = (
                std::mem::take(&mut t.gathered),
                t.looped,
                t.is_ramping_down(),
            );
            let fade = t.params.fade_children;
            let (dice, triggers) = split(host);
            if let Some(u) = triggers {
                u.add_track_triggers(&mut g, ramping, &mut looped, dice);
            }
            let t = self.tracks.get_mut(b).expect("listed");
            t.gathered = g;
            t.looped = looped;
            if fade {
                host.event(Event::ChildrenOpacity {
                    opacity: 1.0,
                    time: 0.25,
                });
            }
        }
    }
}

/// The host's dice and trigger update, borrowed together (a tracker reaches the trigger update
/// through its object).
// D2-15153
fn split(
    host: &mut dyn Host,
) -> (
    &mut auberean_world::chargen::Random,
    Option<&mut crate::triggers::FrameTriggers>,
) {
    host.dice_and_triggers()
}
