//! A body's element and mode, and the change from one mode to another.
//!
//! The mode tracker keeps the behaviors of the mode the body is in (the "from" tracker) and,
//! while it changes mode, those of the mode it goes to (the "to" tracker). A change walks the
//! behavior table's mode graph one step at a time: each step plays its transition behavior and
//! effects and blends from the old tracker to the new over the step's ramp time, at the speed the
//! transition behavior cycles at when it has one. When a step ends, the next starts, until the
//! body is in the mode it was asked for. The body is drawn blending the two trackers by the step's
//! progress.

use std::sync::Arc;

use auberean_assets::animation::{BehaviorKey, BehaviorTable, ModeChange, ModeKey};

use crate::host::{error, Event, Host};
use crate::queries::TableQueries;
use crate::track::{Params, Track};
use crate::tracker::{Behaviors, IDLE, IDLE_FIDGET};
use crate::triggers::trigger;

/// The mode a change of element falls back to when the current mode has no way there.
pub const PEACE: u32 = 0x4000_0001;

/// A body's modes and behaviors.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Modes {
    pub from: Option<Behaviors>,
    pub to: Option<Behaviors>,
    pub table: Option<Arc<BehaviorTable>>,
    /// How far through the current step, 0 to 1.
    pub transition_time: f32,
    /// How fast the step goes, a second.
    pub transition_velocity: f32,
    /// The element and mode the body was asked to be in.
    pub desired_element: u32,
    pub desired_mode: u32,
    /// The body has a behavior table to animate by.
    pub can_animate: bool,
    /// Who waits on the change of mode.
    pub context: u32,
}

fn idle_restart(t: &mut Behaviors, table: &BehaviorTable, host: &mut dyn Host, both: bool) {
    t.do_autonomous_behavior(table, IDLE, host);
    if both {
        t.do_autonomous_behavior(table, IDLE_FIDGET, host);
    }
    t.start_idle = false;
}

impl Modes {
    /// A tracker in the template's starting element and mode, with nothing to animate by.
    // D2-15167
    #[must_use]
    pub fn new(default_element: u32, default_mode: u32) -> Self {
        Self {
            desired_element: default_element,
            desired_mode: default_mode,
            ..Self::default()
        }
    }

    /// Take the behavior table with id `id` (`table`, as found) and start its behaviors in the
    /// desired element and the table's initial mode. A body with no table id cannot animate;
    /// `false` when the table was not found.
    // D2-15155
    pub fn init(
        &mut self,
        id: u32,
        table: Option<Arc<BehaviorTable>>,
        host: &mut dyn Host,
    ) -> bool {
        if id == 0 {
            self.can_animate = false;
            return true;
        }
        let Some(table) = table else {
            self.can_animate = false;
            return false;
        };
        self.can_animate = true;
        self.desired_mode = table.initial_mode;
        if let Some(old) = self.from.take() {
            old.discard(host);
        }
        let mut t = Behaviors::new();
        t.init(&table, self.desired_element, self.desired_mode, None, host);
        self.from = Some(t);
        self.table = Some(table);
        true
    }

    /// Change mode at once: a waiting change is cancelled, the behaviors carry over into the new
    /// mode's tracker, and those the server asked for stop.
    // D2-15156
    pub fn set_mode(&mut self, mode: u32, host: &mut dyn Host) -> bool {
        if self.context != 0 {
            host.event(Event::ModeDone {
                context: self.context,
                error: error::CANCELLED,
            });
            self.context = 0;
        }
        self.transition_time = 0.0;
        self.transition_velocity = 0.0;
        self.desired_mode = mode;
        let Some(table) = self.table.clone() else {
            return true;
        };
        let Some(from) = self.from.as_mut() else {
            return true;
        };
        if self.to.is_none() && from.mode == mode {
            return true;
        }
        if let Some(mut to) = self.to.take() {
            to.stop_all_server_behaviors(&table, host);
            to.discard(host);
        }
        let element = from.element;
        let mut to = Behaviors::new();
        to.init(&table, element, mode, Some(from), host);
        from.stop_all_server_behaviors(&table, host);
        from.destroy(host);
        self.from = Some(to);
        true
    }

    /// Discard the tracker: each behavior tracker is discarded (the effects their behaviors
    /// started stop), the one being left first, and the table let go.
    // D2-15170
    pub fn discard(&mut self, host: &mut dyn Host) {
        if let Some(from) = self.from.take() {
            from.discard(host);
        }
        if let Some(to) = self.to.take() {
            to.discard(host);
        }
        self.table = None;
    }

    /// Both trackers, the one being left first.
    fn trackers(&mut self) -> impl Iterator<Item = &mut Behaviors> {
        self.from.iter_mut().chain(self.to.iter_mut())
    }

    /// Play a behavior nobody waits on, in both trackers while changing mode.
    // D2-15157
    pub fn do_autonomous_behavior(&mut self, behavior: u32, host: &mut dyn Host) -> bool {
        if !self.can_animate {
            return true;
        }
        let Some(table) = self.table.clone() else {
            return true;
        };
        let mut ok = false;
        for t in self.trackers() {
            ok = t.do_autonomous_behavior(&table, behavior, host);
        }
        ok
    }

    /// Let go of a hold and, when asked, play another behavior; whoever waits on the result is told
    /// through the let-go path.
    // D2-15158
    pub fn release_behavior(&mut self, behavior: u32, next: &Params, host: &mut dyn Host) -> bool {
        if !self.can_animate {
            return true;
        }
        let Some(table) = self.table.clone() else {
            return true;
        };
        if let (Some(_), Some(from)) = (&self.to, &mut self.from) {
            from.release_behavior(behavior);
        }
        let Some(t) = self.to.as_mut().or(self.from.as_mut()) else {
            return true;
        };
        t.release_behavior(behavior);
        let track: Option<&mut Track> = if next.behavior == 0 {
            let track = t.track_mut(behavior);
            if let Some(track) = track {
                track.params.context = next.context;
                Some(track)
            } else {
                None
            }
        } else {
            t.do_behavior(&table, next, host);
            t.track_mut(next.behavior)
        };
        if let Some(track) = track {
            track.callback_with_release = true;
        }
        true
    }

    fn idles_after(&mut self, which_to: bool, host: &mut dyn Host) {
        let Some(table) = self.table.clone() else {
            return;
        };
        let t = if which_to {
            self.to.as_mut()
        } else {
            self.from.as_mut()
        };
        if let Some(t) = t {
            if t.start_idle {
                idle_restart(t, &table, host, true);
            }
        }
    }

    /// Stop a behavior in both trackers; the idles start again when nothing the server asked for
    /// is left. Returns what stopping it in the newer tracker returned.
    // D2-15159
    pub fn stop_behavior(&mut self, behavior: u32, host: &mut dyn Host) -> u32 {
        if !self.can_animate {
            return 0;
        }
        let Some(table) = self.table.clone() else {
            return 0;
        };
        let r = if let Some(to) = self.to.as_mut() {
            if let Some(from) = self.from.as_mut() {
                from.stop_behavior(&table, behavior, host);
            }
            to.stop_behavior(&table, behavior, host)
        } else if let Some(from) = self.from.as_mut() {
            from.stop_behavior(&table, behavior, host)
        } else {
            return 0;
        };
        self.idles_after(self.to.is_some(), host);
        r
    }

    /// Stop every behavior a move cancels, in both trackers, the newer first.
    // D2-15160
    pub fn stop_move_to_cancels(&mut self, host: &mut dyn Host) {
        if !self.can_animate {
            return;
        }
        let Some(table) = self.table.clone() else {
            return;
        };
        if let Some(to) = self.to.as_mut() {
            to.stop_move_to_cancels(&table, host);
            if to.start_idle {
                idle_restart(to, &table, host, true);
            }
        }
        if let Some(from) = self.from.as_mut() {
            from.stop_move_to_cancels(&table, host);
            if from.start_idle {
                idle_restart(from, &table, host, true);
            }
        }
    }

    /// Stop what holds a layer, in both trackers.
    // D2-15161
    pub fn stop_layer(&mut self, layer: u32, host: &mut dyn Host) -> u32 {
        if !self.can_animate {
            return 0;
        }
        let Some(table) = self.table.clone() else {
            return 0;
        };
        let mut r = 0;
        for t in self.trackers() {
            r = t.stop_layer(&table, layer, host);
        }
        r
    }

    /// Stop every behavior the server asked for, in both trackers.
    // D2-15162
    pub fn stop_all_server_behaviors(&mut self, host: &mut dyn Host) -> bool {
        if !self.can_animate {
            return false;
        }
        let Some(table) = self.table.clone() else {
            return false;
        };
        for t in self.trackers() {
            t.stop_all_server_behaviors(&table, host);
        }
        true
    }

    /// The element the body is in (the asked one when it cannot animate).
    // D2-15163
    #[must_use]
    pub fn element(&self) -> u32 {
        match (&self.from, self.can_animate) {
            (Some(f), true) => f.element,
            _ => self.desired_element,
        }
    }

    /// Whether a behavior plays: in the tracker being entered while changing mode, else in the
    /// current one unless a step is still under way.
    // D2-15164
    #[must_use]
    pub fn is_doing_behavior(&self, behavior: u32) -> bool {
        if !self.can_animate {
            return false;
        }
        if let Some(to) = &self.to {
            return to.is_doing_behavior(behavior);
        }
        if self.transition_velocity == 0.0 {
            return self
                .from
                .as_ref()
                .is_some_and(|f| f.is_doing_behavior(behavior));
        }
        false
    }

    /// Whether a playing behavior moves the first-person camera.
    // D2-15165
    #[must_use]
    pub fn dynamic_fp_camera(&self) -> bool {
        self.to
            .as_ref()
            .or(self.from.as_ref())
            .is_some_and(Behaviors::any_dynamic_fp_camera)
    }

    /// The behaviors to tell the network about.
    // D2-15166
    #[must_use]
    pub fn network_params(&self) -> Vec<Params> {
        self.to
            .as_ref()
            .or(self.from.as_ref())
            .map(Behaviors::network_params)
            .unwrap_or_default()
    }

    /// The element and mode the body was asked to be in.
    // D2-15168
    #[must_use]
    pub fn desired_mode_state(&self) -> ModeKey {
        ModeKey {
            element: self.desired_element,
            mode: self.desired_mode,
        }
    }

    /// The element and mode the body is in.
    // D2-15171
    #[must_use]
    pub fn mode_state(&self) -> ModeKey {
        match (&self.from, self.can_animate) {
            (Some(f), true) => ModeKey {
                element: f.element,
                mode: f.mode,
            },
            _ => self.desired_mode_state(),
        }
    }

    /// Play a behavior: in both trackers while changing mode, the waiting context going with the
    /// newer one; the idles stop while a behavior the server asked for plays. A body that cannot
    /// animate tells whoever waits at once.
    // D2-15172
    pub fn do_behavior(&mut self, params: &Params, host: &mut dyn Host) -> bool {
        if !self.can_animate {
            host.event(Event::BehaviorDone {
                context: params.context,
                error: error::NONE,
            });
            return true;
        }
        let Some(table) = self.table.clone() else {
            return false;
        };
        let stop_idles = |t: &mut Behaviors, host: &mut dyn Host| {
            if t.stop_idle {
                t.stop_behavior(&table, IDLE, host);
                t.stop_behavior(&table, IDLE_FIDGET, host);
                t.stop_idle = false;
            }
        };
        if let Some(to) = self.to.as_mut() {
            if let Some(from) = self.from.as_mut() {
                let quiet = Params {
                    context: 0,
                    ..params.clone()
                };
                from.do_behavior(&table, &quiet, host);
                stop_idles(from, host);
            }
            let ok = to.do_behavior(&table, params, host);
            stop_idles(to, host);
            return ok;
        }
        let Some(from) = self.from.as_mut() else {
            return false;
        };
        let ok = from.do_behavior(&table, params, host);
        stop_idles(from, host);
        ok
    }

    /// Start a step toward `element` and `mode`: a step under way is finished at once (its waiting
    /// change told it is over); then the first step on the way is taken, with the new mode's
    /// tracker carrying the behaviors over, the step's behavior played and its effects fired.
    /// `false` when there is no way.
    // D2-15173
    pub fn start_transitioning(&mut self, element: u32, mode: u32, host: &mut dyn Host) -> bool {
        let Some(table) = self.table.clone() else {
            return false;
        };
        if self.from.is_none() {
            return false;
        }
        if let Some(to) = self.to.take() {
            if let Some(old) = self.from.replace(to) {
                old.discard(host);
            }
            self.transition_time = 0.0;
            self.transition_velocity = 0.0;
            if self.context != 0 {
                host.event(Event::ModeDone {
                    context: self.context,
                    error: error::NONE,
                });
                self.context = 0;
            }
        }
        let from = self.from.as_ref().expect("checked");
        let mut change = ModeChange {
            from: ModeKey {
                element: from.element,
                mode: from.mode,
            },
            to: ModeKey { element, mode },
        };
        if change.from == change.to {
            self.desired_element = element;
            self.desired_mode = mode;
            return true;
        }
        let Some(step) = table.next_step(&change) else {
            return false;
        };
        change.to = step;
        self.desired_element = element;
        self.desired_mode = mode;
        self.transition_velocity = 1.0 / table.mode_change_ramp_time(&change);
        let mut to = Behaviors::new();
        to.init(&table, step.element, step.mode, self.from.as_mut(), host);
        self.to = Some(to);
        if let Some(fx) = table.transition(&change).copied() {
            if fx.behavior != 0 {
                self.do_behavior(&Params::behavior(fx.behavior), host);
                let key = BehaviorKey {
                    element: step.element,
                    mode: step.mode,
                    behavior: fx.behavior,
                };
                let cv = host.combo_cycle_velocity(&key);
                if cv != 0.0 {
                    self.transition_velocity = cv;
                }
            }
            if let Some(u) = host.triggers() {
                if fx.effect != 0 {
                    u.once.add(fx.effect, trigger(fx.effect, 0.0, 1.0, true));
                }
                if fx.stop_effect != 0 {
                    let mut t = trigger(fx.stop_effect, 1.0, 1.0, true);
                    t.stop = true;
                    u.once.add(fx.stop_effect, t);
                }
            }
        }
        true
    }

    /// Go to another element, in the current mode when there is a way, else in the peaceful one.
    // D2-15174
    pub fn do_element(&mut self, element: u32, host: &mut dyn Host) -> bool {
        if self.from.is_some() && self.table.is_some() && self.desired_element != element {
            let mode = self.desired_mode;
            if !self.start_transitioning(element, mode, host)
                && !self.start_transitioning(element, PEACE, host)
            {
                return false;
            }
        }
        true
    }

    /// Go to another mode; `context` hears when the body is there (at once when it already is, or
    /// cannot animate). A change already waited on is told it was stopped, and the new one fails
    /// at once when there is no way.
    // D2-15175
    pub fn do_mode(&mut self, context: u32, mode: u32, host: &mut dyn Host) -> bool {
        if !self.can_animate {
            self.desired_mode = mode;
            if context != 0 {
                host.event(Event::ModeDone {
                    context,
                    error: error::NONE,
                });
            }
            return true;
        }
        if self.desired_mode == mode {
            if context != 0 {
                if self.context != 0 {
                    host.event(Event::ModeDone {
                        context: self.context,
                        error: error::NONE,
                    });
                    self.context = context;
                    return true;
                }
                host.event(Event::ModeDone {
                    context,
                    error: error::NONE,
                });
            }
            return true;
        }
        if self.context != 0 {
            host.event(Event::ModeDone {
                context: self.context,
                error: error::STOPPED,
            });
        }
        self.context = context;
        let element = self.desired_element;
        if !self.start_transitioning(element, mode, host) {
            if self.context != 0 {
                host.event(Event::ModeDone {
                    context: self.context,
                    error: error::NO_ANIMATION,
                });
                self.context = 0;
            }
            return false;
        }
        true
    }

    /// Advance `dt` seconds: both trackers update (restarting their idles when nothing the
    /// server asked for plays), the step moves on and, once through, the new mode's tracker
    /// becomes the current one (telling whoever waits when the body is where it was asked to be);
    /// a body not yet where it was asked to be takes the next step. The body is then drawn from the
    /// two trackers, and [`Modes::gather_triggers`] closes the frame. `false` when there is
    /// nothing to animate (no table, or no tracker).
    // D2-15176, D2-06441
    pub fn update(&mut self, dt: f32, host: &mut dyn Host) -> bool {
        let Some(table) = self.table.clone() else {
            return false;
        };
        let Some(from) = self.from.as_mut() else {
            return false;
        };
        from.update(&table, dt, host);
        if from.start_idle {
            idle_restart(from, &table, host, true);
            if self.is_doing_behavior(IDLE) {
                if let Some(t) = self.from.as_mut().and_then(|f| f.track_mut(IDLE)) {
                    t.update(dt);
                }
            }
        }
        if let Some(to) = self.to.as_mut() {
            to.update(&table, dt, host);
            if to.start_idle {
                to.do_autonomous_behavior(&table, IDLE, host);
                to.start_idle = false;
                for b in [IDLE, IDLE_FIDGET] {
                    if let Some(t) = to.track_mut(b) {
                        t.update(dt);
                    }
                }
            }
        }
        self.transition_time += dt * self.transition_velocity;
        if self.transition_time >= 1.0 {
            if let Some(old) = self.from.take() {
                old.discard(host);
            }
            self.from = self.to.take();
            if self.context != 0 {
                if let Some(f) = &self.from {
                    if f.mode == self.desired_mode && f.element == self.desired_element {
                        host.event(Event::ModeDone {
                            context: self.context,
                            error: error::NONE,
                        });
                        self.context = 0;
                    }
                }
            }
            self.transition_time = 0.0;
            self.transition_velocity = 0.0;
        }
        if self.transition_time == 0.0 {
            if let Some(f) = &self.from {
                if f.element != self.desired_element || f.mode != self.desired_mode {
                    let (e, m) = (self.desired_element, self.desired_mode);
                    self.start_transitioning(e, m, host);
                }
            }
        }
        true
    }

    /// Gather every playing behavior's triggers, the current mode's first: the end of a frame,
    /// after the body was drawn.
    // D2-15176
    pub fn gather_triggers(&mut self, host: &mut dyn Host) {
        if let Some(f) = self.from.as_mut() {
            f.add_triggers(host);
        }
        if let Some(t) = self.to.as_mut() {
            t.add_triggers(host);
        }
    }
}
