//! **The key-binding back end: `ICIDM` slots 34–39, and the capture and conflict
//! policy the options page drives them with.**
//!
//! Everything under this module already had a home in [`crate::keymap::InputMap`]
//! (`add_mapping`, `unbind_by_key`, `unbind_all_by_action`, `keys_for_action`,
//! `find_conflicting_controls`) and in [`crate::keyfile`] (the `.keymap` reader and writer). What
//! was missing was the layer above them — the six Windows input-manager entry points that take an
//! **input-map id** and find the section, and the `ActionKeyMapOption` policy that decides
//! whether a captured control may become a binding at all. Nothing in this workspace could change a
//! binding.
//!
//! Sources, all in the client itself:
//!
//! | what | where |
//! |---|---|
//! | bind action | creates the map if absent, then adds the mapping |
//! | unbind by key | removes one control from one map |
//! | find keys for action | lists the controls bound to an action |
//! | find conflicting input maps | the maps an action conflicts in |
//! | find conflicting controls | the controls a capture would displace |
//! | unbind all by action | clears every control bound to one action |
//! | the capture rules | the options page's key-hit handler |
//! | applying one | the options page's binding setter |
//!
//! **The one asymmetry worth knowing.** Binding uses the create-map path, so binding into a map
//! that does not exist yet *creates* it;
//! the other five use the plain map lookup and answer "no" for an absent map. Both are reproduced.
//!
//! # What this module is not
//!
//! It is not the options page. Each option row owns its available key-button slots, current
//! bindings, the modal "press a key" dialog and the two
//! overwrite dialogs. [`InputManager::set_binding`] takes the row's slot index as a parameter and
//! does the map surgery; which slot the player clicked, and what to do when the row is already
//! full, is the page's. See the note on [`InputManager::set_binding`].

use crate::spec::{activation, ControlChord};
use crate::{ActionId, DeviceType, InputManager, InputMapId, MAP_BLOCK_KEYBOARD};

/// `DIK_ESCAPE` — the keyboard offset the key-hit handler treats as "cancel". \[verified\]
pub const DIK_ESCAPE: u16 = 0x01;
/// `DIMOFS_X`, the mouse pointer's X axis. Never bindable from the page.
pub const DIMOFS_X: u16 = 0x00;
/// `DIMOFS_Y`.
pub const DIMOFS_Y: u16 = 0x04;
/// `DIMOFS_BUTTON0`, the left mouse button. Bindable only **with** a modifier.
pub const DIMOFS_BUTTON0: u16 = 0x0C;

/// Action **1**, `DoNothing` — the action a freed control is bound to when a rebind takes its key
/// away, and the single most load-bearing constant in this module.
///
/// Oracle: the shipped `ActionMap`'s own enum names, `EnumMapper 0x22000021` — `crate::names`'s
/// action table reads `(0x00000000, "Invalid"), (0x00000001, "DoNothing"), (0x00000002,
/// "PointerX")`. It is **not** bound anywhere in either shipped master keymap, because no shipped
/// keymap binds anything to it: it exists to be written into a *user's* keymap.
///
/// **Why it matters, and why a one-sided test cannot see it.**
/// The merge *adds what is absent*, and merges the user file first and the shipped default
/// after it. So a control the user file does not **mention** gets its shipped default back on the
/// next run. Rebinding *Move Forward* from `DIK_W` to `DIK_F7` and merely deleting the `DIK_W`
/// binding would therefore work for one session and silently come back on the next, with **both**
/// keys walking. The binding setter's
/// `unbind_by_key(old); if !new.is_conflicting(old) { bind_action(old, DoNothing, map) }` is what stops
/// that: `DIK_W -> DoNothing` is written to the `.keymap`, the merge sees `DIK_W` present, and the
/// default is dropped.
pub const DO_NOTHING: ActionId = ActionId(1);

/// The `activation` bits that make the key-capture handler ignore an event outright: `Down` (0x01) and
/// `Analog` (0x80).
///
/// `(activation & 0x81) == 0` is the function's own first test, so a binding is
/// captured on the **release** — which is what makes the modifier state complete. \[verified\]
pub const CAPTURE_IGNORED_MASK: u32 = activation::DOWN | activation::ANALOG;

/// One `(input map, control, action)` a proposed binding would displace.
///
/// The client's own tuple is `pair<ulong, pair<ControlChord, ulong>>` in the array
/// the key-hit handler builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conflict {
    pub input_map: InputMapId,
    pub control: ControlChord,
    pub action: ActionId,
}

/// What the client decided about one captured control.
///
/// Every arm is one of the function's own early returns, in its order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capture {
    /// `activation & 0x81` — a `Down` or `Analog` event. The handler stays registered and waits
    /// for the matching release; **not** a refusal.
    Ignored,
    /// `DIMOFS_X`, `DIMOFS_Y`, or an unmodified `DIMOFS_BUTTON0`. The handler stays registered.
    ///
    /// The left button with no modifier is excluded because it is how the player would click
    /// *out* of the capture dialog; with a modifier it is bindable.
    Rejected,
    /// `DIK_ESCAPE`: unregister the key-hit handler, close the dialog, bind nothing.
    Cancelled,
    /// `is_exactly_equal` against a control already bound to this action — nothing to do, and the
    /// handler returns `true` without touching the map.
    Unchanged,
    /// At least one conflicting action is **not** user-bindable, which opens the "cannot
    /// overwrite" dialog
    /// and the binding is refused outright.
    Refused(Vec<Conflict>),
    /// Conflicts, all of them user-bindable -> the overwrite dialog, which lists
    /// them and asks. Re-run [`InputManager::capture_key_hit`] with `confirmed = true` to get
    /// [`Capture::Ready`] instead.
    NeedsConfirmation {
        control: ControlChord,
        conflicts: Vec<Conflict>,
    },
    /// No conflicts, or the player confirmed -> the binding is applied.
    ///
    /// `control`'s activation has already been forced to `Click` (3), which is step 4 of the
    /// handler and the reason no binding made from this page ever carries `Tap` or `DblClick`.
    Ready {
        control: ControlChord,
        conflicts: Vec<Conflict>,
    },
}

impl InputManager {
    /// Bind an action: create the input map if it does not exist, then add the
    /// mapping to it.
    ///
    /// **Creates** the section if it does not exist, unlike every other entry point here.
    pub fn bind_action(&mut self, control: ControlChord, action: ActionId, map: InputMapId) {
        self.keymap
            .create_input_map(map)
            .add_mapping(control, action);
    }

    /// Unbind by key -- one control in one map.
    ///
    /// Returns whether the map existed, matching the client's unbind-by-key result.
    pub fn unbind_by_key(&mut self, control: &ControlChord, map: InputMapId) -> bool {
        match self.keymap.section_mut(map) {
            Some(s) => {
                s.unbind_by_key(control);
                true
            }
            None => false,
        }
    }

    /// Unbind all by action -- every control bound to one
    /// action in one map. This is the options row's *Clear all* button.
    pub fn unbind_all_by_action(&mut self, action: ActionId, map: InputMapId) -> bool {
        match self.keymap.section_mut(map) {
            Some(s) => {
                s.unbind_all_by_action(action);
                true
            }
            None => false,
        }
    }

    /// Find the keys for an action -- what the options row shows, and what it
    /// re-reads after every change.
    ///
    /// An input-map id of 0 answers nothing, exactly as the client's non-zero map check does.
    #[must_use]
    pub fn find_keys_for_action(&self, action: ActionId, map: InputMapId) -> Vec<ControlChord> {
        if map.0 == 0 {
            return Vec::new();
        }
        self.keymap
            .section(map)
            .map(|s| s.keys_for_action(action))
            .unwrap_or_default()
    }

    /// Find the conflicting input maps.
    /// Return input maps that conflict with this binding.
    ///
    /// The set of maps that can be registered **at the same time** as this one, which is the only
    /// set in which two bindings for the same control can actually fight. It is why the melee,
    /// missile and magic maps may all bind `DIK_DELETE` to different actions: at most one of them
    /// is registered at a time, so they do not list each other.
    #[must_use]
    pub fn find_conflicting_input_maps(&self, map: InputMapId) -> &[InputMapId] {
        self.action_map.conflicting_input_maps(map)
    }

    /// Find the conflicting controls, which asks the input map for them.
    #[must_use]
    pub fn find_conflicting_controls(
        &self,
        control: &ControlChord,
        map: InputMapId,
    ) -> Vec<(ControlChord, ActionId)> {
        if map.0 == 0 {
            return Vec::new();
        }
        self.keymap
            .section(map)
            .map(|s| s.find_conflicting_controls(control))
            .unwrap_or_default()
    }

    /// The key-hit handler -- the whole capture policy, as a verdict.
    ///
    /// The client routes **every** control here before any input
    /// map is consulted, because it registers as input handler `0x20`
    /// ([`crate::dispatch::handler_flags::KEY_HIT`]), a single exclusive slot. So this function
    /// sees controls no map binds, and its job is to say which of them may become a binding.
    ///
    /// The steps are the client's, in its order:
    ///
    /// 1. `activation & 0x81` (`Down`, `Analog`) → [`Capture::Ignored`]. The binding is taken on
    ///    the **release** so the modifier state is complete.
    /// 2. mouse `DIMOFS_X`/`DIMOFS_Y`, and `DIMOFS_BUTTON0` with `metamode == 0` →
    ///    [`Capture::Rejected`].
    /// 3. keyboard offset 1 (`DIK_ESCAPE`) → [`Capture::Cancelled`].
    /// 4. activation is forced to `Click` (3).
    /// 5. `is_exactly_equal` against the action's existing controls → [`Capture::Unchanged`].
    /// 6. conflicts: `find_conflicting_controls` over every map in `find_conflicting_input_maps`,
    ///    de-duplicated (the client's already-in-the-array guard) and with map
    ///    [`MAP_BLOCK_KEYBOARD`] dropped — it is a barrier and carries no bindings, so the drop is
    ///    belt and braces rather than an observable.
    /// 7. any conflicting action not user-bindable → [`Capture::Refused`]; otherwise
    ///    [`Capture::NeedsConfirmation`].
    /// 8. `confirmed` skips step 7 — it is the client's own already-asked flag, set when
    ///    the overwrite dialog comes back Yes and the handler is re-entered.
    #[must_use]
    pub fn capture_key_hit(
        &self,
        map: InputMapId,
        action: ActionId,
        control: ControlChord,
        confirmed: bool,
    ) -> Capture {
        // 1.
        if control.activation & CAPTURE_IGNORED_MASK != 0 {
            return Capture::Ignored;
        }
        let device = self.keymap.device_type_of(control.control);
        let offset = control.control.offset();
        // 2.
        if device == Some(DeviceType::Mouse)
            && (offset == DIMOFS_X
                || offset == DIMOFS_Y
                || (offset == DIMOFS_BUTTON0 && control.meta_mode == 0))
        {
            return Capture::Rejected;
        }
        // 3.
        if device == Some(DeviceType::Keyboard) && offset == DIK_ESCAPE {
            return Capture::Cancelled;
        }
        // 4.
        let control = ControlChord::new(control.control, control.meta_mode, activation::CLICK);
        // 5.
        if self
            .find_keys_for_action(action, map)
            .iter()
            .any(|existing| existing.is_exactly_equal(&control))
        {
            return Capture::Unchanged;
        }
        // 6.
        let mut conflicts: Vec<Conflict> = Vec::new();
        for other in self.find_conflicting_input_maps(map) {
            if *other == MAP_BLOCK_KEYBOARD {
                continue;
            }
            for (c, a) in self.find_conflicting_controls(&control, *other) {
                let entry = Conflict {
                    input_map: *other,
                    control: c,
                    action: a,
                };
                if !conflicts.contains(&entry) {
                    conflicts.push(entry);
                }
            }
        }
        // 7 and 8.
        if !conflicts.is_empty() && !confirmed {
            if let Some(bad) = conflicts
                .iter()
                .find(|c| !self.action_map.is_user_bindable(c.input_map, c.action))
            {
                let _ = bad;
                return Capture::Refused(conflicts);
            }
            return Capture::NeedsConfirmation { control, conflicts };
        }
        Capture::Ready { control, conflicts }
    }

    /// Apply a [`Capture::Ready`].
    ///
    /// ```text
    /// if (qc.key == 0xFFFFFFFF) return false;
    /// if (qc.activation == 0)   return false;
    /// for (m : find_conflicting_input_maps(map))
    ///     for (c : find_conflicting_controls(qc, m)) unbind_by_key(c, m);
    /// if (slot is within the row's current keys) {
    ///     unbind_by_key(current[slot]);   // the key this row's button showed until now
    ///     current[slot] = qc;
    ///     unbind_all_by_action(action);
    ///     for (k : current) bind_action(k, action, map);
    /// } else {
    ///     bind_action(qc, action, map);
    ///     current.push(qc);
    /// }
    /// Notify the character UI to refresh its action-key mapping.
    /// ```
    ///
    /// `slot` is the index of the row key-button the player clicked. `Some(i)` with
    /// `i < find_keys_for_action(...).len()` is a **replacement** — the old key stops firing the
    /// action, which is the half of a rebind a stored-table assertion cannot see. Anything else
    /// **adds** a second key for the same action, which is a real and different operation: the
    /// shipped rows carry several keys per action.
    ///
    /// The `bind_action(old, DoNothing, map)` in the middle of the replace branch is not
    /// housekeeping — see [`DO_NOTHING`]. Without it a rebind lasts exactly one session.
    ///
    /// **The `unbind_by_key(old)` before it is correct, is kept, and is unfalsifiable here.**
    /// Mutating it away leaves every rebinding test in this crate and in the client
    /// green. This is a case where the code and test are
    /// right but the explanation of which line does the
    /// work is wrong*. Two later lines already remove `old -> action` on their own:
    /// adding a mapping **replaces in place** for a key that is already present, so
    /// `bind_action(old, DoNothing)` overwrites the binding rather than adding beside it, and
    /// `unbind_all_by_action` then sweeps whatever is left of the action. It is the client's literal
    /// first act and it is load-bearing in the full-row branch this build does not have, so it
    /// stays — deleting code because no test misses it is how faithful transcription is lost.
    ///
    /// **One thing here is the page's and is deliberately not reproduced.** The client's
    /// out-of-range branch compares the row's key-button capacity with its current binding count
    /// and, when the row is full, recycles the head entry the same way. The capacity belongs to the
    /// options-row layout, so that branch belongs with the options page.
    ///
    /// Returns whether anything was bound.
    pub fn set_binding(
        &mut self,
        map: InputMapId,
        action: ActionId,
        slot: Option<usize>,
        control: ControlChord,
    ) -> bool {
        use crate::spec::ControlCode;
        if control.control == ControlCode::INVALID || control.activation == 0 {
            return false;
        }
        // Displace whatever else answers to this control in a map that can be registered at the
        // same time as this one.
        let others: Vec<InputMapId> = self.find_conflicting_input_maps(map).to_vec();
        for other in others {
            for (c, _) in self.find_conflicting_controls(&control, other) {
                self.unbind_by_key(&c, other);
            }
        }
        let mut keys = self.find_keys_for_action(action, map);
        match slot.filter(|i| *i < keys.len()) {
            Some(i) => {
                let old = keys[i];
                self.unbind_by_key(&old, map);
                // The freed control is bound to [`DO_NOTHING`] rather than left absent, which is
                // what makes the rebind survive a save and reload. See that constant.
                if !control.is_conflicting(&old) {
                    self.bind_action(old, DO_NOTHING, map);
                }
                keys[i] = control;
                self.unbind_all_by_action(action, map);
                for k in &keys {
                    self.bind_action(*k, action, map);
                }
            }
            None => self.bind_action(control, action, map),
        }
        true
    }
}

// The tests for this module are in the `dat` tier, driven off the **shipped** `ActionMap`
// and both master input maps read from the retail dats rather than off a fabricated one: the
// conflict table (16 entries) and the user-bindable flags are exactly the
// data this policy consults, and a hand-built stand-in would be asserting against itself.
