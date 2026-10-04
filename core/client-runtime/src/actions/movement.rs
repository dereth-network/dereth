//! Movement input: motion-command encoding, the three command
//! lists, run/walk, autorun and the jump charge/release.
//!
//! It covers the command interpreter's action handler and motion setter, the keyboard-command
//! handler, the list selection, the add and nuke steps, the hold-key application, the
//! current-movement application, and the run-lock and auto-run latches.
//!
//! This module owns the **input** half. What a motion command *does* — the movement manager, the
//! interpolation, the speeds and the server messages — lives in the movement engine, which is why
//! nothing here reaches into `dereth-animation`.

use dereth_client_contract::actions::Action;

use crate::actions::ActionId;

/// The motion commands the input layer produces.
pub mod command {
    pub const READY: u32 = 0x4100_0003;
    pub const WALK_FORWARD: u32 = 0x4500_0005;
    pub const WALK_BACKWARDS: u32 = 0x4500_0006;
    pub const TURN_RIGHT: u32 = 0x6500_000D;
    pub const TURN_LEFT: u32 = 0x6500_000E;
    pub const SIDE_STEP_RIGHT: u32 = 0x6500_000F;
    pub const SIDE_STEP_LEFT: u32 = 0x6500_0010;
    pub const AUTO_RUN: u32 = 0x0900_00C7;
    pub const HOLD_RUN: u32 = 0x8500_0001;
    pub const HOLD_SIDESTEP: u32 = 0x8500_0002;
    pub const MOVE_TO: u32 = 0x2500_003B;

    /// Hold-key pseudo-command.
    pub const BIT_HOLD_KEY: u32 = 0x8000_0000;
    /// A motion/state command.
    pub const BIT_MOTION: u32 = 0x4000_0000;
    /// **Not** a motion command — `handle_keyboard_command` rejects it outright. `AutoRun` is the
    /// one command special-cased *before* the test.
    pub const BIT_NOT_MOTION: u32 = 0x0800_0000;
    /// Belongs on the substate list.
    pub const BIT_SUBSTATE: u32 = 0x0400_0000;
}

/// The movement action ids, which are the shared vocabulary's.
pub use dereth_client_contract::actions::movement as action;

/// Which of the three lists a command belongs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandList {
    Turn,
    Sidestep,
    Substate,
    /// A transient action such as `Ready`, which is on no list.
    None,
}

/// The list selection.
#[must_use]
pub const fn which_list(cmd: u32) -> CommandList {
    match cmd {
        command::TURN_RIGHT | command::TURN_LEFT => CommandList::Turn,
        command::SIDE_STEP_RIGHT | command::SIDE_STEP_LEFT => CommandList::Sidestep,
        _ => {
            if cmd & command::BIT_MOTION != 0 && cmd & command::BIT_SUBSTATE != 0 {
                CommandList::Substate
            } else {
                CommandList::None
            }
        }
    }
}

/// The `CmdStruct` the client builds on the stack and the command handler consumes: a command,
/// an optional float extent and an optional
/// "start" flag.
///
/// Only the *reader* side is known; how the motion setter fills the buffer on the stack is not.
/// The observable contract — command, optional float
/// extent, optional start flag, in that order — is verified; the exact packing is inferred. This is
/// the shape the reader consumes, so a malformed motion command would show up here first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CmdStruct {
    pub command: u32,
    /// Read by `handle_keyboard_command` only for `AutoRun`, defaulting to 1.0 when absent.
    pub extent: Option<f32>,
    /// `0` = key up, non-zero = key down.
    pub start: Option<bool>,
}

/// One decision of the command interpreter's action handler.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MovementAction {
    /// `SetMotion(command, start)` — the `CmdStruct` handed to the keyboard-command handler.
    SetMotion(CmdStruct),
    /// `set_hold_run(start)`.
    SetHoldRun(bool),
    /// `CommenceJump` — the key went down.
    CommenceJump,
    /// `DoJump(true)` — the key came up.
    DoJump,
    /// Not consumed; falls through to the global handler list.
    NotHandled,
}

/// The command interpreter's action handler.
///
/// `emote_command` is the emote input-action hash -- 87 emote actions plus the four stances.
#[must_use]
pub fn on_action(
    event: &Action,
    emote_command: impl Fn(ActionId) -> Option<u32>,
) -> MovementAction {
    let motion = |cmd: u32, start: bool| {
        MovementAction::SetMotion(CmdStruct {
            command: cmd,
            extent: None,
            start: Some(start),
        })
    };
    let start = event.is_start();
    match event.id {
        action::MOVE_FORWARD => motion(command::WALK_FORWARD, start),
        action::MOVE_BACKWARD => motion(command::WALK_BACKWARDS, start),
        // Ready is always "start", whichever edge produced the event.
        action::STOP_MOVING => motion(command::READY, true),
        action::STRAFE_RIGHT => motion(command::SIDE_STEP_RIGHT, start),
        action::STRAFE_LEFT => motion(command::SIDE_STEP_LEFT, start),
        action::TURN_RIGHT => motion(command::TURN_RIGHT, start),
        action::TURN_LEFT => motion(command::TURN_LEFT, start),
        // Toggle type 2, so the start flag alternates and the command itself is always a "start".
        action::AUTORUN => motion(command::AUTO_RUN, true),
        action::JUMP => {
            if start {
                MovementAction::CommenceJump
            } else {
                MovementAction::DoJump
            }
        }
        action::TOGGLE_RUN_WALK => MovementAction::SetHoldRun(start),
        // This client's hold sidestep is the keyboard command's own hold key, held with its key.
        dereth_client_contract::actions::ActionId(
            dereth_client_contract::actions::dereth::MOVEMENT_HOLD_SIDESTEP,
        ) => motion(command::HOLD_SIDESTEP, start),
        other => match emote_command(other) {
            Some(cmd) => motion(cmd, true),
            None => MovementAction::NotHandled,
        },
    }
}

/// The three command lists, with "remove and resume the new head" — what makes overlapping movement
/// keys behave. A single "current command" variable does not reproduce it.
#[derive(Debug)]
pub struct CommandLists {
    pub turn: Vec<CommandEntry>,
    pub sidestep: Vec<CommandEntry>,
    pub substate: Vec<CommandEntry>,
    /// `transient_state`.
    pub transient_state: bool,
    /// `auto_run`.
    pub auto_run: bool,
    /// `hold_run`.
    pub hold_run: bool,
    /// `hold_sidestep`.
    pub hold_sidestep: bool,
    /// The controlled-by-server flag.
    ///
    /// Set by [`Self::lose_control_to_server`] when the server sends this player a
    /// **non-autonomous** movement buffer, cleared by [`Self::take_control_from_server`]. While it
    /// is set the body is doing what the server told it to and the three lists are held in
    /// reserve; the moment the server's motions finish, the per-frame step hands control back and
    /// `apply_current_movement` re-issues the heads. That is the whole of "the character starts
    /// running again after transitioning to combat stance".
    ///
    /// Retail initialises it
    /// to **1** when a new player is created, and every loss of keyboard focus re-loses it.
    /// This build starts it **false**, because it does not model that creation path.
    ///
    /// The `false` start is safe only because [`Self::handle_keyboard_command`] makes the
    /// client's unconditional take-control call. Without it, the first non-autonomous buffer the
    /// server sends would latch the flag, no key press could clear it, and every key release would
    /// be routed to [`KeyboardCommand::NonAutonomous`]. Only the new-player half of the deviation
    /// remains.
    pub controlled_by_server: bool,
    /// Autonomy level, initialised to **2**.
    ///
    /// Both losing and taking control are no-ops at level 0, and the autonomous-state check is
    /// `autonomy_level != 2`. The setter refuses anything above 2.
    pub autonomy_level: u32,
}

impl Default for CommandLists {
    /// Matches the retail defaults except for the one field named as a deviation on
    /// [`Self::controlled_by_server`].
    fn default() -> Self {
        Self {
            turn: Vec::new(),
            sidestep: Vec::new(),
            substate: Vec::new(),
            transient_state: false,
            auto_run: false,
            hold_run: false,
            hold_sidestep: false,
            controlled_by_server: false,
            autonomy_level: 2,
        }
    }
}

/// The three facts command dispatch reads from state outside the command lists.
#[derive(Debug, Clone, Copy)]
pub struct CommandEnv {
    /// Whether the server currently controls movement. A release received in this state leaves the
    /// command list but is not issued: the release goes through the nuke step. The clear-all-commands
    /// step is not used here.
    pub controlled_by_server: bool,
    /// The *Run as Default Movement* character option
    /// (action `0x1000007B`), which Shift **XORs** with rather than overriding.
    pub ui_toggles_run: bool,
    /// The speed recorded on the command-list entry.
    pub speed: f32,
}

impl Default for CommandEnv {
    fn default() -> Self {
        Self {
            controlled_by_server: false,
            ui_toggles_run: false,
            speed: 1.0,
        }
    }
}

/// What one [`CommandLists::handle_keyboard_command`] did, after the lists have been updated.
///
/// The variants are the branches of `handle_keyboard_command`'s own dispatch, and the caller needs
/// them apart: [`Self::Ignored`] is the one that must be handed back to the rest of the
/// input-handler list rather than swallowed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyboardCommand {
    /// `AutoRun` — the one command special-cased *before* the `0x08000000` test.
    /// `message` is `set_auto_run`'s message-window line, `None` when the state did not change.
    AutoRun {
        speed: f32,
        message: Option<&'static str>,
    },
    /// `cmd & 0x08000000` and not `AutoRun`: **not a motion command**, dropped on the floor.
    Ignored,
    /// The non-autonomous move-player call — a release while the server has control.
    NonAutonomous(u32),
    /// `set_hold_run(start)`; `run` is the XOR of the hold key with *Run as Default Movement*.
    HoldRun { run: bool },
    /// `set_hold_sidestep(start)`.
    HoldSidestep(bool),
    /// `move_player(cmd, start)` — the command the bookkeeping accepted, after
    /// `apply_hold_keys_to_command`.
    ///
    /// `new_forward` is [`CommandLists::add_command`]'s own return, carried out rather than
    /// discarded: the *"new forward movement"* edge the new-forward-movement handler
    /// raises on. The run-lock half of that override is this
    /// crate's and has already run inside `add_command`; the **other** half is the combat
    /// system's automatic-attack abort, which needs a combat system and therefore belongs
    /// to the caller. Without it a mapped Backward press walks the body backwards and leaves the
    /// repeat attack armed.
    ///
    /// It is `true` only for an accepted **press** that `add_command` pushed onto the substate
    /// list, or for a listless motion command (`Ready`, the four stances). A release, a resume, a
    /// turn, a sidestep, a `MoveTo` and a refused command are all `false`.
    Move {
        command: u32,
        start: bool,
        new_forward: bool,
    },
    /// The bookkeeping refused: a release that removed nothing, or removed the last entry and had
    /// no new head to hand back. The lists still changed; nothing new is issued.
    Refused,
}

/// One entry of a command list. `head_is_mouse` distinguishes a mouse-originated head from a
/// keyboard one, so that `apply_current_movement` re-issues it
/// differently: a mouse command is *not* re-`add_command`ed, only re-applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CommandEntry {
    pub command: u32,
    pub speed: f32,
    pub head_is_mouse: bool,
    pub hold_run: bool,
}

impl CommandLists {
    fn list_mut(&mut self, which: CommandList) -> Option<&mut Vec<CommandEntry>> {
        match which {
            CommandList::Turn => Some(&mut self.turn),
            CommandList::Sidestep => Some(&mut self.sidestep),
            CommandList::Substate => Some(&mut self.substate),
            CommandList::None => None,
        }
    }

    /// Add a command to its list.
    ///
    /// ```text
    /// list = which_list(cmd)
    /// if (list == NULL) {
    ///     if ((cmd & 0x40000000) && !(cmd & 0x04000000)) {
    ///         clear_all_commands();
    ///         if (cmd != Ready) transient_state = 1;
    ///     }
    /// } else {
    ///     add_command(list, ...);                    // the push happens FIRST
    ///     if (cmd & 0x40000000) {
    ///         if (list == &substate_list) new_forward_movement_handler();
    ///         transient_state = 0;
    ///     }
    /// }
    /// ```
    ///
    /// **The call in both arms is the new-forward-movement handler, not `clear_all_commands`.**
    /// Its behavior is `set_auto_run(0)` plus an automatic-attack abort, so nothing here empties
    /// a list it has just pushed to, because nothing here empties a list at all. It is *"any new
    /// forward movement cancels the run lock"* — autorun is cancelled in **three** places; the
    /// other two are the input layer's `turn_off_run_lock` and
    /// [`CommandLists::lose_control_to_server`]'s `set_auto_run(0, 0)`.
    ///
    /// So the `set_auto_run(0)` half happens **here**, and the return says whether it did. The
    /// automatic-attack abort belongs to combat input and is not modelled in this crate —
    /// **but the edge is carried out of it**. If this `bool` stopped at [`Self::bookkeep`], the
    /// forward-movement handler's other statement, the automatic-attack abort, would have no
    /// producer and a mapped Backward press would leave a repeated melee attack armed. It travels
    /// on [`KeyboardCommand::Move::new_forward`] to the one caller that holds a combat system.
    pub fn add_command(&mut self, e: CommandEntry) -> bool {
        let which = which_list(e.command);
        let new_forward = match which {
            CommandList::None => {
                if e.command & command::BIT_MOTION != 0 && e.command & command::BIT_SUBSTATE == 0 {
                    self.transient_state = e.command != command::READY;
                    true
                } else {
                    false
                }
            }
            other => {
                if let Some(list) = self.list_mut(other) {
                    list.retain(|x| x.command != e.command);
                    list.insert(0, e);
                }
                if e.command & command::BIT_MOTION != 0 {
                    self.transient_state = false;
                    other == CommandList::Substate
                } else {
                    false
                }
            }
        };
        if new_forward {
            // `set_auto_run(0, …)`. Its own `apply_current_movement` tail is the caller's, because
            // the command that triggered this is about to be issued anyway.
            let _ = self.set_auto_run(false);
        }
        new_forward
    }

    /// Removes the command from its list and, if the
    /// list is not empty afterwards, hands back the new head so the caller re-issues it.
    ///
    /// **That is the "last key wins, previous key resumes" behaviour**: holding W then also holding
    /// X and releasing X resumes forward motion.
    ///
    /// The first half of the answer is `nuke_command`'s **`int` return** — *accepted*, not
    /// *removed*. They differ: a removal made while `transient_state` is set, or off the substate
    /// list while autorunning, returns **0** and the caller issues nothing at all. The second half
    /// is the new head, `None` when the list emptied — and an empty list is still an *accepted*
    /// nuke, which is what makes a release stop the movement.
    ///
    /// It answers *accepted*, not *removed*, because *removed* is not the return the client
    /// tests.
    pub fn nuke_command(&mut self, cmd: u32) -> (bool, Option<CommandEntry>) {
        let which = which_list(cmd);
        let (transient, auto_run) = (self.transient_state, self.auto_run);
        let Some(list) = self.list_mut(which) else {
            return (false, None);
        };
        let before = list.len();
        list.retain(|e| e.command != cmd);
        let removed = list.len() != before;
        if !removed || transient || (auto_run && which == CommandList::Substate) {
            return (false, None);
        }
        let head = self.list_mut(which).and_then(|l| l.first().copied());
        (true, head)
    }

    /// Clears all three command lists.
    pub fn clear_all_commands(&mut self) {
        self.turn.clear();
        self.sidestep.clear();
        self.substate.clear();
    }

    /// Clears keyboard commands from all three lists -- the first
    /// three statements of losing keyboard focus.
    ///
    /// It is **not** [`Self::clear_all_commands`], and the difference is the whole point of the
    /// name. The clear's first loop walks the head and stops dead at the mouse element,
    /// unlinking and deleting each head entry until it reaches the mouse command; if anything is
    /// left, a second loop unlinks and deletes every entry after the head.
    ///
    /// So the list a focus loss leaves
    /// behind is exactly `[the mouse command]`, or empty when the mouse holds nothing. A gesture
    /// the mouse is driving survives an alt-tab; the keys do not.
    ///
    /// This build has no mouse-command pointer: the flag lives per entry, as
    /// [`CommandEntry::head_is_mouse`], written by [`Self::add_command`] from
    /// the command insertion path's from-mouse argument. The first entry carrying it is
    /// the one the mouse-command pointer would point at, so "keep the first mouse entry and drop
    /// the rest" is the same list, reached from the same fact.
    pub fn clear_keyboard_commands(&mut self) {
        fn one(list: &mut Vec<CommandEntry>) {
            match list.iter().position(|e| e.head_is_mouse) {
                Some(i) => {
                    let mouse = list[i];
                    list.clear();
                    list.push(mouse);
                }
                None => list.clear(),
            }
        }
        one(&mut self.turn);
        one(&mut self.sidestep);
        one(&mut self.substate);
    }

    /// Bookkeeps a command and modifies it if necessary. Its whole body is:
    ///
    /// ```text
    /// if (cmd == MoveTo)  return 1;
    /// if (start)        { add_command(...); return 1; }
    /// return nuke_command(&cmd, &start, ...);                // it MODIFIES both
    /// ```
    ///
    /// "And modifies it if necessary" is the point: `nuke_command` rewrites `cmd` **and** `start`
    /// in place when it hands back a new head, and it sets `start = 1` — so a release of the
    /// second key becomes a *press* of the first. That is why the answer here is a pair.
    ///
    /// Answering `None` whenever the list emptied would lose two distinct outcomes at once: the
    /// resume's `start = 1`, and the ordinary "the last key came up, stop moving" —
    /// `nuke_command` returns **1** with `cmd` and `start` untouched when it removed the only
    /// entry, and that is the release that reaches `move_player`. `None` means only what the
    /// client's `0` means: nothing was accepted. The third member is [`Self::add_command`]'s own
    /// return, the *"new forward movement"* edge. `add_command` is only reached on the `start`
    /// leg, so every other leg answers `false` — which is exactly retail's shape, because
    /// `nuke_command` does not call the new-forward-movement handler and the `MoveTo` leg never
    /// reaches either.
    pub fn bookkeep(
        &mut self,
        cmd: u32,
        start: bool,
        entry: CommandEntry,
    ) -> Option<(u32, bool, bool)> {
        if cmd == command::MOVE_TO {
            return Some((cmd, start, false));
        }
        if start {
            let new_forward = self.add_command(entry);
            return Some((cmd, true, new_forward));
        }
        let (accepted, head) = self.nuke_command(cmd);
        if !accepted {
            return None;
        }
        match head {
            Some(h) => Some((h.command, true, false)),
            None => Some((cmd, false, false)),
        }
    }

    /// While `hold_sidestep`, `TurnRight`
    /// becomes `SideStepRight` and `TurnLeft` becomes `SideStepLeft`.
    #[must_use]
    pub const fn apply_hold_keys_to_command(&self, cmd: u32) -> u32 {
        if self.hold_sidestep {
            match cmd {
                command::TURN_RIGHT => return command::SIDE_STEP_RIGHT,
                command::TURN_LEFT => return command::SIDE_STEP_LEFT,
                _ => {}
            }
        }
        cmd
    }

    /// Re-asserts the whole movement state,
    /// which the per-frame step does **every frame** while the server has control. The client does
    /// not trust the edge events.
    #[must_use]
    pub fn apply_current_movement(&self) -> AppliedMovement {
        let base = if self.auto_run {
            Some(command::WALK_FORWARD)
        } else if let Some(h) = self.substate.first() {
            Some(h.command)
        } else if self.transient_state {
            None
        } else {
            Some(command::READY)
        };
        AppliedMovement {
            base,
            turn: self.turn.first().map(|h| h.command),
            sidestep: self.sidestep.first().map(|h| h.command),
        }
    }

    /// Sets the auto-run latch and returns the message-window text when the state
    /// actually changed — two **hard-coded UTF-16 literals**, not string-table entries.
    ///
    /// `apply_current_movement` is not inside the *on* branch and `send_movement_event` is not
    /// the tail. In the client's own auto-run latch the on-branch call takes control back from
    /// the server, and the tail is `if (send_event) apply_current_movement`. `toggle_auto_run`
    /// passes `send_event = true`, so **a toggle re-asserts the movement in both directions** —
    /// which is what makes turning the run lock *off* actually stop the character. The caller runs
    /// that tail; this returns only the text.
    pub fn set_auto_run(&mut self, on: bool) -> Option<&'static str> {
        if on == self.auto_run {
            return None;
        }
        self.auto_run = on;
        self.transient_state = false;
        Some(if on { "AutoRun ON" } else { "AutoRun OFF" })
    }

    /// Toggles auto-run by applying `set_auto_run(!auto_run, true)`.
    pub fn toggle_auto_run(&mut self) -> Option<&'static str> {
        self.set_auto_run(!self.auto_run)
    }

    /// Loses movement control to the server.
    /// What the client does:
    ///
    /// ```text
    ///   if (autonomy_level == 0) return;           ; level 0: nothing happens
    ///   controlled_by_server = 1;
    ///   set_auto_run(0, send_event = 0);
    ///   then finish_jump;
    /// ```
    ///
    /// **`set_auto_run(0)` is the whole reason auto-run and a held key behave differently.** The
    /// run lock is cancelled here; the three command lists are *not* touched. So when
    /// [`Self::take_control_from_server`] later re-issues the heads, a held `W` is still on the
    /// substate list and starts the character running again, while auto-run has nothing left to
    /// re-issue and the character stays where the server stopped it. That exactly matches the
    /// observed stance-transition behavior.
    ///
    /// `send_event` is **0**, so the auto-run latch's `apply_current_movement` tail does **not**
    /// run, but the message-window line does because it is built and sent before that gate. So
    /// retail prints **"AutoRun OFF"** when a stance change stops an
    /// auto-running character. The text is returned here for the caller to show on the
    /// message-window channel `0x1A`, exactly as [`Self::set_auto_run`]'s other
    /// callers do.
    ///
    /// `finish_jump` belongs to combat input and is not modelled in this crate.
    pub fn lose_control_to_server(&mut self) -> Option<&'static str> {
        if self.autonomy_level == 0 {
            return None;
        }
        self.controlled_by_server = true;
        self.set_auto_run(false)
    }

    /// The control-retake gate.
    ///
    /// ```text
    /// if (player && enabled && controlled_by_server
    ///     && !motions_pending(player)
    ///     && !is_moving_to(player)) {
    ///     if (substate_list.head == NULL && turn_list.head == NULL
    ///         && sidestep_list.head == NULL && !auto_run) return;
    ///     take_control_from_server();
    /// }
    /// ```
    ///
    /// Two things make this the mechanism rather than a detail. **`motions_pending` is the
    /// delay**: the retake waits for the stance-change animation to finish, which is why the stop
    /// "doesn't seem instantaneous". And **the second `if` is the auto-run/held-key split**: with
    /// the lists empty and `auto_run` already cleared by [`Self::lose_control_to_server`], the
    /// function returns without retaking control at all, so the character stays server-stopped.
    ///
    /// `enabled` is the interpreter's disable/enable flag and is not modelled
    /// here; the caller holds the `player` half.
    #[must_use]
    pub fn can_take_control_from_server(&self, motions_pending: bool, is_moving_to: bool) -> bool {
        if !self.controlled_by_server || motions_pending || is_moving_to {
            return false;
        }
        !(self.substate.is_empty()
            && self.turn.is_empty()
            && self.sidestep.is_empty()
            && !self.auto_run)
    }

    /// Take control back from the server.
    ///
    /// ```text
    ///   return unless controlled by the server
    ///   return if the autonomy level is 0
    ///   return if the player is dead
    ///   clear controlled_by_server
    ///   set the player's last_move_was_autonomous
    ///   stop_completely (player, 1)
    ///   stop_interpolating (player)
    ///   set_hold_run (hold_run)
    ///   then apply_current_movement
    /// ```
    ///
    /// This function owns only the flag; the four body-side statements and the two tail calls
    /// belong to the caller, which is the only side that holds the player's physics body. Returns whether
    /// the transfer happened, so the caller knows whether to run them.
    ///
    /// The player's override first aborts automatic attack when it is active, then applies
    /// this movement update. The combat-system abort is not modelled here.
    ///
    /// The boolean answer is the caller's too.
    pub fn take_control_from_server(&mut self) -> bool {
        if !self.controlled_by_server || self.autonomy_level == 0 {
            return false;
        }
        self.controlled_by_server = false;
        true
    }

    /// Sets the sidestep hold state, the other hold key.
    ///
    /// It has no binding in the shipped keymap (`HoldSidestep` `0x85000002` is reachable only from
    /// a console command), so nothing presses it in a normal session; it is here because
    /// [`Self::handle_keyboard_command`]'s dispatch has an arm for it and an arm with no
    /// implementation behind it is the shape this project calls *transcribed and unwired*.
    pub fn set_hold_sidestep(&mut self, on: bool) {
        self.hold_sidestep = on;
    }

    /// Handles a keyboard motion command, the entry point
    /// `crate::movement::MovementAction::SetMotion` is on its way to, and the **only** thing that
    /// writes the three lists in a running client.
    ///
    /// [`on_action`] decides; this call is what turns the decision into list changes rather than a
    /// bool per direction. The dispatch:
    ///
    /// ```text
    /// if (!active) return;
    /// if (player == NULL) return;
    /// cmd = arg.command
    /// if (cmd == 0x090000C7) {                       // AutoRun
    ///     autorun_speed = next float argument, or 1.0 if absent
    ///     toggle_auto_run();
    ///     send_movement_event();
    ///     return;
    /// }
    /// if (cmd & 0x08000000) return;                  // any other non-motion command
    /// start = next ulong argument
    /// if (controlled_by_server && !start) { nuke_command(&cmd, &start, ...); return; }
    /// take_control_from_server();
    /// if      (cmd == 0x85000001) set_hold_run(start);
    /// else if (cmd == 0x85000002) set_hold_sidestep(start);
    /// else if (bookkeep(&cmd, &start)) {
    ///     apply_hold_keys_to_command(&cmd);
    ///     move_player(cmd, start);
    /// ..
    /// }
    /// if (!is_standing_still()) send_movement_event();
    /// ```
    ///
    /// Two details are easy to get wrong: the unconditional call before the hold-key branch takes
    /// control back from the server, rather than setting the sidestep hold state; and
    /// `apply_hold_keys_to_command` runs **after** the bookkeeping, on the command that was
    /// accepted, so the list keeps `TurnLeft` while what is *issued* is `SideStepLeft`.
    ///
    /// # The two statements a release under server control depends on
    ///
    /// ```text
    ///   if (controlled_by_server && start == 0) {
    ///       nuke_command(...);
    ///       return;                                ; ...and RETURN, nothing is issued
    ///   }
    ///   take_control_from_server();
    /// ..then compare the command against 0x85000001
    /// ```
    ///
    /// Neither can be observed while nothing sets [`Self::controlled_by_server`]; the server's
    /// non-autonomous movement buffers do, so both are required. See that field's own note.
    ///
    /// The take-control call's answer is **discarded here, exactly as the client discards it** — the
    /// call that follows reads nothing back. The body-side half of taking control back
    /// (stopping completely, `set_hold_run`, `apply_current_movement`)
    /// lives on the caller, which is the only side holding the physics body; it learns the transfer
    /// happened by reading [`Self::controlled_by_server`] across this call, the same way
    /// [`Self::take_control_from_server`]'s own `bool` serves [`Self::can_take_control_from_server`]'s
    /// caller.
    pub fn handle_keyboard_command(&mut self, c: &CmdStruct, env: &CommandEnv) -> KeyboardCommand {
        let cmd = c.command;
        if cmd == command::AUTO_RUN {
            // `handle_keyboard_command` reads the extent as `autorun_speed` and defaults it to 1.0
            // when the buffer carries none, which is every keyboard press: the motion setter writes
            // the start flag and no float.
            let speed = c.extent.unwrap_or(1.0);
            let message = self.toggle_auto_run();
            return KeyboardCommand::AutoRun { speed, message };
        }
        if cmd & command::BIT_NOT_MOTION != 0 {
            return KeyboardCommand::Ignored;
        }
        // `start` is `None` only for commands this decode never produces; a missing edge is a
        // press, which is the start flag's default.
        let start = c.start.unwrap_or(true);
        if env.controlled_by_server && !start {
            // The nuke step, with
            // both out-parameters written and then thrown away because the arm returns
            // before reaching `move_player`. **The removal is not thrown away**: the
            // released key leaves its list, so the `apply_current_movement` that ends the eventual
            // retake re-issues the heads *without* it. A release that never left the list would
            // be a key that can never be let go of.
            let _ = self.nuke_command(cmd);
            return KeyboardCommand::NonAutonomous(cmd);
        }
        // Taking control from the server, which
        // ends in the interpreter's own retake. Unconditional, before the hold-key branch,
        // and the only thing in the client
        // that clears `controlled_by_server` when the retake gate in the per-frame apply
        // cannot: with three empty lists and no run lock
        // that gate returns without retaking, which is the state a teleport or a stance change
        // taken while standing leaves behind.
        let _ = self.take_control_from_server();
        match cmd {
            command::HOLD_RUN => KeyboardCommand::HoldRun {
                run: self.set_hold_run(start, env.ui_toggles_run),
            },
            command::HOLD_SIDESTEP => {
                self.set_hold_sidestep(start);
                KeyboardCommand::HoldSidestep(start)
            }
            _ => {
                let entry = CommandEntry {
                    command: cmd,
                    speed: env.speed,
                    head_is_mouse: false,
                    hold_run: self.hold_run,
                };
                match self.bookkeep(cmd, start, entry) {
                    Some((accepted, accepted_start, new_forward)) => KeyboardCommand::Move {
                        command: self.apply_hold_keys_to_command(accepted),
                        start: accepted_start,
                        // The new-forward-movement effect travels to the caller
                        // that owns the combat-input response.
                        new_forward,
                    },
                    None => KeyboardCommand::Refused,
                }
            }
        }
    }

    /// The run-lock latch.
    ///
    /// ```text
    /// hold_run = (param != 0)
    /// run = (hold_run != 0) != (ui_toggles_run != 0)      // XOR
    /// ```
    ///
    /// `ui_toggles_run` is the *Run as Default Movement* character option
    /// (action `0x1000007B`). Shift therefore **inverts** the option rather than always meaning
    /// "run". The default binding for Toggle
    /// Run/Walk is `DIK_LSHIFT`, which is also the Shift meta key — a meta key still generates its
    /// own control event, so both roles work at once.
    pub fn set_hold_run(&mut self, param: bool, ui_toggles_run: bool) -> bool {
        self.hold_run = param;
        self.hold_run != ui_toggles_run
    }
}

/// The three heads `apply_current_movement` re-issues. `None` for `turn`/`sidestep` means
/// stopping that list's head movement; `None` for `base` means the transient state is held and
/// nothing is re-applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedMovement {
    pub base: Option<u32>,
    pub turn: Option<u32>,
    pub sidestep: Option<u32>,
}

/// `MIN_JUMP_EXTENT` -- the combat system's jump takes
/// `max(power-bar level, MIN_JUMP_EXTENT)` when the level is at least 0.001.
pub const JUMP_POWER_THRESHOLD: f32 = 0.001;

/// The jump command's extent rule.
#[must_use]
pub fn jump_extent(power_bar_level: f32, min_jump_extent: f32) -> f32 {
    if power_bar_level >= JUMP_POWER_THRESHOLD {
        power_bar_level.max(min_jump_extent)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(action: ActionId, start: bool) -> Action {
        if start {
            Action::begin(action)
        } else {
            Action::end(action).with_extent(1.0)
        }
    }

    /// Oracle: the recovered movement-input behavior §1's action → motion-command table.
    #[test]
    fn the_movement_actions_map_to_the_documented_commands() {
        let none = |_: ActionId| None;
        let cmd = |a: ActionId, s: bool| match on_action(&ev(a, s), none) {
            MovementAction::SetMotion(c) => Some((c.command, c.start)),
            _ => None,
        };
        assert_eq!(
            cmd(action::MOVE_FORWARD, true),
            Some((0x4500_0005, Some(true)))
        );
        assert_eq!(
            cmd(action::MOVE_BACKWARD, false),
            Some((0x4500_0006, Some(false)))
        );
        // Stop Moving is always "start", even on the release.
        assert_eq!(
            cmd(action::STOP_MOVING, false),
            Some((0x4100_0003, Some(true)))
        );
        assert_eq!(
            cmd(action::STRAFE_RIGHT, true),
            Some((0x6500_000F, Some(true)))
        );
        assert_eq!(
            cmd(action::STRAFE_LEFT, true),
            Some((0x6500_0010, Some(true)))
        );
        assert_eq!(
            cmd(action::TURN_RIGHT, true),
            Some((0x6500_000D, Some(true)))
        );
        assert_eq!(
            cmd(action::TURN_LEFT, true),
            Some((0x6500_000E, Some(true)))
        );
        assert_eq!(cmd(action::AUTORUN, false), Some((0x0900_00C7, Some(true))));
        assert_eq!(
            on_action(&ev(action::JUMP, true), none),
            MovementAction::CommenceJump
        );
        assert_eq!(
            on_action(&ev(action::JUMP, false), none),
            MovementAction::DoJump
        );
        assert_eq!(
            on_action(&ev(action::TOGGLE_RUN_WALK, true), none),
            MovementAction::SetHoldRun(true)
        );
        assert_eq!(
            on_action(&ev(ActionId(0x9999), true), none),
            MovementAction::NotHandled
        );
        // An emote action resolves through the hash and is always a "start".
        let emotes = |a: ActionId| (a == ActionId(0x1000_0094)).then_some(0x4100_0003);
        assert_eq!(
            on_action(&ev(ActionId(0x1000_0094), true), emotes),
            MovementAction::SetMotion(CmdStruct {
                command: 0x4100_0003,
                extent: None,
                start: Some(true)
            })
        );
    }

    /// Oracle: the recovered movement-input behavior §1's `WhichList` table.
    #[test]
    fn which_list_routes_by_the_documented_bits() {
        assert_eq!(which_list(command::TURN_RIGHT), CommandList::Turn);
        assert_eq!(which_list(command::TURN_LEFT), CommandList::Turn);
        assert_eq!(which_list(command::SIDE_STEP_RIGHT), CommandList::Sidestep);
        assert_eq!(which_list(command::SIDE_STEP_LEFT), CommandList::Sidestep);
        // WalkForward 0x45000005 has 0x40000000 and 0x04000000 set.
        assert_eq!(which_list(command::WALK_FORWARD), CommandList::Substate);
        assert_eq!(which_list(command::WALK_BACKWARDS), CommandList::Substate);
        // Ready 0x41000003 has 0x40000000 but not 0x04000000.
        assert_eq!(which_list(command::READY), CommandList::None);
    }

    /// Releasing the second key resumes the first.
    #[test]
    fn releasing_the_second_key_resumes_the_first() {
        let mut l = CommandLists::default();
        let e = |c| CommandEntry {
            command: c,
            speed: 1.0,
            head_is_mouse: false,
            hold_run: false,
        };
        // Hold W, then also hold X.
        assert_eq!(
            l.bookkeep(command::WALK_FORWARD, true, e(command::WALK_FORWARD)),
            Some((command::WALK_FORWARD, true, true))
        );
        assert_eq!(
            l.bookkeep(command::WALK_BACKWARDS, true, e(command::WALK_BACKWARDS)),
            Some((command::WALK_BACKWARDS, true, true))
        );
        // Releasing X hands back W -- as a *press*, which is the whole point of the resume.
        assert_eq!(
            l.bookkeep(command::WALK_BACKWARDS, false, e(command::WALK_BACKWARDS)),
            Some((command::WALK_FORWARD, true, false))
        );
        assert_eq!(l.apply_current_movement().base, Some(command::WALK_FORWARD));
        // Releasing the last one is still accepted, and it is accepted as a *release*: that is the
        // `MovePlayer(cmd, 0)` that stops the character.
        assert_eq!(
            l.bookkeep(command::WALK_FORWARD, false, e(command::WALK_FORWARD)),
            Some((command::WALK_FORWARD, false, false))
        );
        assert!(l.substate.is_empty());
        // A release of something that was never held removes nothing and is refused outright.
        assert_eq!(
            l.bookkeep(command::WALK_FORWARD, false, e(command::WALK_FORWARD)),
            None
        );
    }

    /// Oracle: the recovered movement-input behavior §3's description of `ApplyCurrentMovement`.
    #[test]
    fn apply_current_movement_prefers_autorun_then_substate_then_ready() {
        let mut l = CommandLists::default();
        assert_eq!(
            l.apply_current_movement(),
            AppliedMovement {
                base: Some(command::READY),
                turn: None,
                sidestep: None
            }
        );
        l.transient_state = true;
        assert_eq!(l.apply_current_movement().base, None);
        let _ = l.add_command(CommandEntry {
            command: command::WALK_FORWARD,
            speed: 1.0,
            head_is_mouse: false,
            hold_run: false,
        });
        assert!(
            !l.transient_state,
            "a substate push clears the transient state"
        );
        assert_eq!(l.apply_current_movement().base, Some(command::WALK_FORWARD));
        l.auto_run = true;
        assert_eq!(l.apply_current_movement().base, Some(command::WALK_FORWARD));
        l.turn.insert(
            0,
            CommandEntry {
                command: command::TURN_LEFT,
                speed: 1.0,
                head_is_mouse: false,
                hold_run: false,
            },
        );
        assert_eq!(l.apply_current_movement().turn, Some(command::TURN_LEFT));
    }

    /// Oracle: the recovered movement-input behavior §3 — Shift XORs with *Run as Default Movement*.
    /// Do not hard-code Shift = run.
    #[test]
    fn shift_xors_with_the_run_as_default_option() {
        let mut l = CommandLists::default();
        // Option off: holding Shift runs.
        assert!(l.set_hold_run(true, false));
        assert!(!l.set_hold_run(false, false));
        // Option on: holding Shift walks.
        assert!(!l.set_hold_run(true, true));
        assert!(l.set_hold_run(false, true));
    }

    /// Oracle: the recovered movement-input behavior §3 — the two hard-coded strings, and no message
    /// when the state does not change.
    #[test]
    fn autorun_reports_only_on_a_change() {
        let mut l = CommandLists::default();
        assert_eq!(l.set_auto_run(false), None);
        assert_eq!(l.toggle_auto_run(), Some("AutoRun ON"));
        assert_eq!(l.set_auto_run(true), None);
        assert_eq!(l.toggle_auto_run(), Some("AutoRun OFF"));
    }

    /// The keyboard command dispatch takes each documented arm.
    #[test]
    fn the_keyboard_command_dispatch_takes_each_documented_arm() {
        let env = CommandEnv::default();
        let cmd = |c: u32, start: Option<bool>| CmdStruct {
            command: c,
            extent: None,
            start,
        };

        // 1. AutoRun is special-cased *before* the 0x08000000 test, and it toggles.
        let mut l = CommandLists::default();
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::AUTO_RUN, Some(true)), &env),
            KeyboardCommand::AutoRun {
                speed: 1.0,
                message: Some("AutoRun ON")
            }
        );
        assert!(l.auto_run);
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::AUTO_RUN, Some(true)), &env),
            KeyboardCommand::AutoRun {
                speed: 1.0,
                message: Some("AutoRun OFF")
            }
        );
        assert!(
            !l.auto_run,
            "the command is always a start; the *state* is what alternates"
        );
        // The extent, when the buffer carries one, is `autorun_speed`.
        assert_eq!(
            l.handle_keyboard_command(
                &CmdStruct {
                    command: command::AUTO_RUN,
                    extent: Some(0.5),
                    start: Some(true)
                },
                &env
            ),
            KeyboardCommand::AutoRun {
                speed: 0.5,
                message: Some("AutoRun ON")
            }
        );

        // 2. Any *other* 0x08000000 command is dropped -- and that is the arm whose answer the
        //    caller must put back rather than swallow.
        let mut l = CommandLists::default();
        assert_eq!(
            l.handle_keyboard_command(&cmd(0x0900_0001, Some(true)), &env),
            KeyboardCommand::Ignored
        );

        let mut l = CommandLists {
            controlled_by_server: true,
            ..CommandLists::default()
        };
        let server = CommandEnv {
            controlled_by_server: true,
            ..CommandEnv::default()
        };
        let _ = l.add_command(CommandEntry {
            command: command::WALK_FORWARD,
            speed: 1.0,
            head_is_mouse: false,
            hold_run: false,
        });
        assert_eq!(
            l.substate.len(),
            1,
            "the premise: the key is held before it is released"
        );
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::WALK_FORWARD, Some(false)), &server),
            KeyboardCommand::NonAutonomous(command::WALK_FORWARD)
        );
        assert!(
            l.substate.is_empty(),
            "NukeCommand removed it even though nothing was issued"
        );
        assert!(
            l.controlled_by_server,
            "and this arm returns before the take-control call"
        );
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::WALK_FORWARD, Some(true)), &server),
            KeyboardCommand::Move {
                command: command::WALK_FORWARD,
                start: true,
                new_forward: true,
            }
        );
        assert!(
            !l.controlled_by_server,
            "the press took control back before it was bookkept"
        );
        // ..and now that it has, the release is an ordinary one again.
        assert_eq!(
            l.handle_keyboard_command(
                &cmd(command::WALK_FORWARD, Some(false)),
                &CommandEnv {
                    controlled_by_server: l.controlled_by_server,
                    ..CommandEnv::default()
                }
            ),
            KeyboardCommand::Move {
                command: command::WALK_FORWARD,
                start: false,
                new_forward: false,
            },
            "and it is MovePlayer(cmd, 0) -- the statement that stops the character"
        );

        // 4. The two hold keys. Shift XORs with *Run as Default Movement*.
        let mut l = CommandLists::default();
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::HOLD_RUN, Some(true)), &env),
            KeyboardCommand::HoldRun { run: true }
        );
        let opt = CommandEnv {
            ui_toggles_run: true,
            ..CommandEnv::default()
        };
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::HOLD_RUN, Some(true)), &opt),
            KeyboardCommand::HoldRun { run: false },
            "with the option on, holding Shift walks"
        );
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::HOLD_SIDESTEP, Some(true)), &env),
            KeyboardCommand::HoldSidestep(true)
        );
        assert!(l.hold_sidestep);

        // 5. `ApplyHoldKeysToCommand` runs on the *accepted* command, after the bookkeeping: the
        //    list keeps TurnLeft and what is issued is SideStepLeft.
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::TURN_LEFT, Some(true)), &env),
            KeyboardCommand::Move {
                command: command::SIDE_STEP_LEFT,
                start: true,
                new_forward: false,
            }
        );
        assert_eq!(
            l.turn.first().map(|e| e.command),
            Some(command::TURN_LEFT),
            "the list holds the command that was pressed, not the one that was issued"
        );

        // 6. A release that empties the list is still *accepted* -- it is the `MovePlayer(cmd, 0)`
        //    that stops the character. `Refused` is reserved for a nuke that removed nothing.
        let mut l = CommandLists::default();
        let _ = l.handle_keyboard_command(&cmd(command::WALK_FORWARD, Some(true)), &env);
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::WALK_FORWARD, Some(false)), &env),
            KeyboardCommand::Move {
                command: command::WALK_FORWARD,
                start: false,
                new_forward: false,
            }
        );
        assert!(l.substate.is_empty());
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::WALK_FORWARD, Some(false)), &env),
            KeyboardCommand::Refused,
            "a release of a key that was never held removes nothing"
        );

        // 7. `AddCommand`'s `HandleNewForwardMovement` half: any new forward movement cancels the
        //    run lock. This is the *command interpreter's* half of the rule; the input layer's is
        // and the recovered movement-input behavior §9 says all three are
        //    needed.
        let mut l = CommandLists::default();
        assert!(l.set_auto_run(true).is_some());
        let _ = l.handle_keyboard_command(&cmd(command::WALK_FORWARD, Some(true)), &env);
        assert!(
            !l.auto_run,
            "a forward command on the substate list cancels autorun"
        );
        assert!(l.set_auto_run(true).is_some());
        let _ = l.handle_keyboard_command(&cmd(command::READY, Some(true)), &env);
        assert!(
            !l.auto_run,
            "and so does `Ready`, which is on no list at all"
        );
        // A *turn* does not: it is neither a substate push nor a listless motion command.
        assert!(l.set_auto_run(true).is_some());
        let _ = l.handle_keyboard_command(&cmd(command::TURN_LEFT, Some(true)), &env);
        assert!(l.auto_run, "turning while autorunning is how you steer");
    }

    /// The new forward movement edge reaches the caller.
    #[test]
    fn the_new_forward_movement_edge_reaches_the_caller() {
        let env = CommandEnv::default();
        let cmd = |c: u32, start: bool| CmdStruct {
            command: c,
            extent: None,
            start: Some(start),
        };
        let edge = |l: &mut CommandLists, c: u32, start: bool| match l
            .handle_keyboard_command(&cmd(c, start), &env)
        {
            KeyboardCommand::Move { new_forward, .. } => Some(new_forward),
            _ => None,
        };

        // `WalkBackwards` is `0x45000006`: motion **and** substate, so
        // `WhichList` puts it on the substate list and the add step's `if (list == &SubstateList)`
        // is taken. Retail's Backward press is a *new forward movement* in the interpreter's own
        // vocabulary, which is why it aborts the automatic attack.
        let mut l = CommandLists::default();
        assert_eq!(edge(&mut l, command::WALK_BACKWARDS, true), Some(true));
        // ..and letting it go is not.
        assert_eq!(edge(&mut l, command::WALK_BACKWARDS, false), Some(false));
        assert_eq!(edge(&mut l, command::WALK_FORWARD, true), Some(true));
        assert_eq!(edge(&mut l, command::WALK_FORWARD, false), Some(false));

        // Turning and strafing are their own lists, and the add step calls nothing for them.
        let mut l = CommandLists::default();
        assert_eq!(edge(&mut l, command::TURN_LEFT, true), Some(false));
        assert_eq!(edge(&mut l, command::SIDE_STEP_RIGHT, true), Some(false));

        // `Ready` is on no list and is `0x41000003` -- motion, not substate -- so it takes the
        // `list == NULL` arm's `(cmd & 0x40000000) && !(cmd & 0x04000000)` and is an edge too.
        let mut l = CommandLists::default();
        assert_eq!(edge(&mut l, command::READY, true), Some(true));

        // A release that removes nothing is `Refused`, and refuses to be an edge.
        let mut l = CommandLists::default();
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::WALK_BACKWARDS, false), &env),
            KeyboardCommand::Refused
        );

        // The resume: hold A, press D, release D. `NukeCommand` hands back the head as a press
        // and never reaches `AddCommand`, so the resumed turn raises no edge either.
        let mut l = CommandLists::default();
        assert_eq!(edge(&mut l, command::TURN_LEFT, true), Some(false));
        assert_eq!(edge(&mut l, command::TURN_RIGHT, true), Some(false));
        assert_eq!(edge(&mut l, command::TURN_RIGHT, false), Some(false));
    }

    /// Releasing the second turn key resumes the first through the dispatch.
    #[test]
    fn releasing_the_second_turn_key_resumes_the_first_through_the_dispatch() {
        let env = CommandEnv::default();
        let cmd = |c: u32, start: bool| CmdStruct {
            command: c,
            extent: None,
            start: Some(start),
        };
        let mut l = CommandLists::default();

        assert_eq!(
            l.handle_keyboard_command(&cmd(command::TURN_LEFT, true), &env),
            KeyboardCommand::Move {
                command: command::TURN_LEFT,
                start: true,
                new_forward: false
            }
        );
        assert_eq!(l.apply_current_movement().turn, Some(command::TURN_LEFT));
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::TURN_RIGHT, true), &env),
            KeyboardCommand::Move {
                command: command::TURN_RIGHT,
                start: true,
                new_forward: false
            }
        );
        assert_eq!(l.apply_current_movement().turn, Some(command::TURN_RIGHT));
        // The release hands back the head that was underneath, **as a press** -- `NukeCommand`
        // sets `start = 1` on the resume, which is what turns a key-up into "keep turning left".
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::TURN_RIGHT, false), &env),
            KeyboardCommand::Move {
                command: command::TURN_LEFT,
                start: true,
                new_forward: false
            }
        );
        assert_eq!(
            l.apply_current_movement().turn,
            Some(command::TURN_LEFT),
            "releasing the second key resumes the first"
        );
        // ..and releasing the first one really does stop the turn: accepted, as a release.
        assert_eq!(
            l.handle_keyboard_command(&cmd(command::TURN_LEFT, false), &env),
            KeyboardCommand::Move {
                command: command::TURN_LEFT,
                start: false,
                new_forward: false
            }
        );
        assert_eq!(l.apply_current_movement().turn, None);
    }

    /// Oracle: the recovered movement-input behavior §3 — `ApplyHoldKeysToCommand`.
    #[test]
    fn hold_sidestep_turns_turning_into_strafing() {
        let mut l = CommandLists::default();
        assert_eq!(
            l.apply_hold_keys_to_command(command::TURN_RIGHT),
            command::TURN_RIGHT
        );
        l.hold_sidestep = true;
        assert_eq!(
            l.apply_hold_keys_to_command(command::TURN_RIGHT),
            command::SIDE_STEP_RIGHT
        );
        assert_eq!(
            l.apply_hold_keys_to_command(command::TURN_LEFT),
            command::SIDE_STEP_LEFT
        );
        assert_eq!(
            l.apply_hold_keys_to_command(command::WALK_FORWARD),
            command::WALK_FORWARD
        );
    }
}
