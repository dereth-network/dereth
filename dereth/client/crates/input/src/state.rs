//! `ActionState`, the six toggle types, catch-up auto-repeat and the run lock.
//!
//! This covers the client's action-fire path, the activate, deactivate and toggle steps, the
//! action start, the run-lock release, the per-frame tick and the extent query.

use dereth_primitives::LocalTime;

use crate::actionmap::ToggleType;
use crate::spec::ControlCode;
use crate::{ActionId, CallbackId, InputMapId};

/// The global holding the Autorun action id.
pub const MOVEMENT_RUN_LOCK: ActionId = ActionId(0x30);

/// The three movement actions whose *start* cancels autorun: Move Forward, Move Backward and
/// Stop Moving. calls `turn_off_run_lock` for exactly these.
pub const RUN_LOCK_CANCELLING_ACTIONS: [ActionId; 3] =
    [ActionId(0x29), ActionId(0x2A), ActionId(0x2B)];

/// Result of adding or removing a key press from an action state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStateChange {
    /// The first control to hold this action.
    Started,
    /// Another control joined or left, but the action is still held.
    Updated,
    /// The last control released.
    Stopped,
    /// Nothing changed.
    None,
}

/// One physical control holding an action, with its own extent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SingleKeyInfo {
    pub control: ControlCode,
    pub extent: f32,
}

/// `ActionState`, 40 bytes in the client.
#[derive(Debug, Clone)]
pub struct ActionState {
    pub action: ActionId,
    pub time_action_began: LocalTime,
    pub repeat_count: u32,
    pub toggle: ToggleType,
    pub callback: Option<CallbackId>,
    /// Every control currently holding the action.
    pub keys: Vec<SingleKeyInfo>,
}

impl ActionState {
    #[must_use]
    pub fn new(
        action: ActionId,
        toggle: ToggleType,
        callback: Option<CallbackId>,
        now: LocalTime,
    ) -> Self {
        Self {
            action,
            time_action_began: now,
            repeat_count: 0,
            toggle,
            callback,
            keys: Vec::new(),
        }
    }

    /// Add a control press. A control already present has its extent updated.
    pub fn add_key_press(&mut self, control: ControlCode, extent: f32) -> ActionStateChange {
        if let Some(k) = self.keys.iter_mut().find(|k| k.control == control) {
            let changed = (k.extent - extent).abs() > f32::EPSILON;
            k.extent = extent;
            return if changed {
                ActionStateChange::Updated
            } else {
                ActionStateChange::None
            };
        }
        let first = self.keys.is_empty();
        self.keys.push(SingleKeyInfo { control, extent });
        if first {
            ActionStateChange::Started
        } else {
            ActionStateChange::Updated
        }
    }

    /// Remove one key press and report the resulting state change.
    pub fn remove_key_press(&mut self, control: ControlCode) -> ActionStateChange {
        let before = self.keys.len();
        self.keys.retain(|k| k.control != control);
        if self.keys.len() == before {
            return ActionStateChange::None;
        }
        if self.keys.is_empty() {
            ActionStateChange::Stopped
        } else {
            ActionStateChange::Updated
        }
    }

    /// The extent of the entry with the largest **absolute**
    /// value, sign preserved, so an analog axis pushed hardest wins.
    #[must_use]
    pub fn extent(&self) -> f32 {
        let mut best = 0.0f32;
        for k in &self.keys {
            if k.extent.abs() > best.abs() {
                best = k.extent;
            }
        }
        best
    }
}

/// The two repeat constants, sampled once at startup from `SystemParametersInfoA` and **never
/// refreshed**.
#[derive(Debug, Clone, Copy)]
pub struct RepeatTiming {
    /// The key-repeat delay -- `(SPI_GETKEYBOARDDELAY + 1) * 0.25` s, so 0.25 ... 1.0.
    pub delay: f64,
    /// The key-repeat speed.
    pub speed: f64,
}

impl RepeatTiming {
    /// The input manager's own constructor values.
    ///
    /// `SPI_GETKEYBOARDSPEED` `v`: `v >= 32 -> 1/30 s`; `v == 0 -> 0.4 s`;
    /// else `0.4 - v * 0.011827956989247313`.
    #[must_use]
    pub fn from_system_parameters(keyboard_delay: u32, keyboard_speed: u32) -> Self {
        let delay = f64::from(keyboard_delay + 1) * 0.25;
        let speed = if keyboard_speed >= 32 {
            1.0 / 30.0
        } else if keyboard_speed == 0 {
            0.4
        } else {
            0.4 - f64::from(keyboard_speed) * 0.011_827_956_989_247_313
        };
        Self { delay, speed }
    }
}

impl Default for RepeatTiming {
    /// The Windows defaults: `SPI_GETKEYBOARDDELAY` 1 and `SPI_GETKEYBOARDSPEED` 31.
    fn default() -> Self {
        Self::from_system_parameters(1, 31)
    }
}

/// What one repeat sweep decided for one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepeatFire {
    pub action: ActionId,
    pub repeat_delta: u32,
    pub repeat_total: u32,
}

/// The active action-state table plus the repeat sweep.
#[derive(Debug, Default)]
pub struct ActionStates {
    states: Vec<ActionState>,
    pub repeat: RepeatTiming,
}

impl ActionStates {
    #[must_use]
    pub fn get(&self, action: ActionId) -> Option<&ActionState> {
        self.states.iter().find(|s| s.action == action)
    }

    pub fn get_mut(&mut self, action: ActionId) -> Option<&mut ActionState> {
        self.states.iter_mut().find(|s| s.action == action)
    }

    /// Whether an action is in progress.
    #[must_use]
    pub fn is_action_in_progress(&self, action: ActionId) -> bool {
        self.get(action).is_some()
    }

    pub fn insert(&mut self, s: ActionState) {
        self.states.retain(|e| e.action != s.action);
        self.states.push(s);
    }

    pub fn remove(&mut self, action: ActionId) -> Option<ActionState> {
        let i = self.states.iter().position(|s| s.action == action)?;
        Some(self.states.remove(i))
    }

    /// Callback removal's second half — every `ActionState` whose
    /// callback is this object goes too, so a destroyed panel cannot leave a held action behind.
    pub fn remove_by_callback(&mut self, cb: CallbackId) {
        self.states.retain(|s| s.callback != Some(cb));
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.states.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ActionState> {
        self.states.iter()
    }

    /// The per-frame tick's repeat half.
    ///
    /// **Toggle 4** — once `now >= began + delay`,
    /// `n = trunc((now - (began + delay)) / speed) + 1`; when `n > repeat_count` the difference is
    /// dispatched. Repeats are therefore **catch-up**: a stalled frame delivers several at once.
    ///
    /// **Toggle 5** — one dispatch every frame, `repeat_delta = 1`.
    pub fn sweep(&mut self, now: LocalTime) -> Vec<RepeatFire> {
        let (delay, speed) = (self.repeat.delay, self.repeat.speed);
        let mut out = Vec::new();
        for s in &mut self.states {
            match s.toggle {
                ToggleType::HoldRepeat => {
                    let start = s.time_action_began.0 + delay;
                    if now.0 < start {
                        continue;
                    }
                    // trunc toward zero, which is what the original's float->int helper does; the
                    // value is non-negative here so trunc and floor agree.
                    let n = dereth_primitives::num::to_i32_f64((now.0 - start) / speed) + 1;
                    let n = u32::try_from(n).unwrap_or(0);
                    if n > s.repeat_count {
                        let delta = n - s.repeat_count;
                        s.repeat_count = n;
                        out.push(RepeatFire {
                            action: s.action,
                            repeat_delta: delta,
                            repeat_total: n,
                        });
                    }
                }
                ToggleType::HoldContinuous => {
                    s.repeat_count += 1;
                    out.push(RepeatFire {
                        action: s.action,
                        repeat_delta: 1,
                        repeat_total: s.repeat_count,
                    });
                }
                _ => {}
            }
        }
        out
    }
}

/// What `fire_action_event` decided to dispatch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActionDispatch {
    pub action: ActionId,
    pub input_map: InputMapId,
    pub toggle: ToggleType,
    pub extent: f32,
    pub start: bool,
    pub callback: Option<CallbackId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered input pipeline §5 — the largest-absolute rule, sign preserved.
    #[test]
    fn extent_is_the_largest_absolute() {
        let mut s = ActionState::new(ActionId(1), ToggleType::Hold, None, LocalTime(0.0));
        s.add_key_press(ControlCode(1), 0.5);
        s.add_key_press(ControlCode(2), -0.9);
        s.add_key_press(ControlCode(3), 0.7);
        assert!((s.extent() - -0.9).abs() < 1e-6);
    }

    /// Oracle: the recovered input pipeline §5 and trap 5 — a stalled frame delivers several repeats at
    /// once, with `repeat_delta` carrying the difference.
    #[test]
    fn repeat_is_catch_up() {
        let mut states = ActionStates {
            repeat: RepeatTiming {
                delay: 0.5,
                speed: 0.1,
            },
            ..ActionStates::default()
        };
        let mut s = ActionState::new(ActionId(0x16), ToggleType::HoldRepeat, None, LocalTime(0.0));
        s.add_key_press(ControlCode(1), 1.0);
        states.insert(s);

        // Before the delay: nothing.
        assert!(states.sweep(LocalTime(0.4)).is_empty());
        // One tick past the delay.
        let a = states.sweep(LocalTime(0.55));
        assert_eq!(
            a,
            vec![RepeatFire {
                action: ActionId(0x16),
                repeat_delta: 1,
                repeat_total: 1
            }]
        );
        // A stalled frame: 0.5 s of repeats arrive together. n = trunc(0.55/0.1)+1 = 6.
        let b = states.sweep(LocalTime(1.05));
        assert_eq!(
            b,
            vec![RepeatFire {
                action: ActionId(0x16),
                repeat_delta: 5,
                repeat_total: 6
            }]
        );
    }

    /// Oracle: the recovered input pipeline §5 — toggle 5 re-fires every frame with delta 1.
    #[test]
    fn toggle_five_refires_every_frame() {
        let mut states = ActionStates::default();
        states.insert(ActionState::new(
            ActionId(9),
            ToggleType::HoldContinuous,
            None,
            LocalTime(0.0),
        ));
        assert_eq!(states.sweep(LocalTime(0.001))[0].repeat_total, 1);
        assert_eq!(states.sweep(LocalTime(0.002))[0].repeat_total, 2);
        assert_eq!(states.sweep(LocalTime(0.003))[0].repeat_delta, 1);
    }

    /// Oracle: the recovered input pipeline §5's table of the key-repeat speed.
    #[test]
    fn repeat_constants_match_the_documented_formulae() {
        let fast = RepeatTiming::from_system_parameters(0, 31);
        assert!((fast.delay - 0.25).abs() < 1e-12);
        assert!((fast.speed - (0.4 - 31.0 * 0.011_827_956_989_247_313)).abs() < 1e-12);
        assert!((RepeatTiming::from_system_parameters(3, 32).speed - 1.0 / 30.0).abs() < 1e-12);
        assert!((RepeatTiming::from_system_parameters(3, 0).speed - 0.4).abs() < 1e-12);
        assert!((RepeatTiming::from_system_parameters(3, 0).delay - 1.0).abs() < 1e-12);
    }
}
