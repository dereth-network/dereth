//! The client's device input: devices and gamepads, window-message modelling, key bindings and the
//! keymap file, the input-map stack and the toggle and repeat machine, ending in actions.
//!
//! **Depends on** `dereth-primitives`, the contract's action vocabulary
//! (`dereth-client-contract`) and the dat decoders (`dereth-assets`), which read the shipped action
//! map and master input maps' bytes that this crate turns into its own input maps. **Used by** the client, the UI crates (`dereth-ui`,
//! `dereth-ui-screens`) and the client's test kit.
//!
//! **Must never** decide what an action does, or reach the runtime (`cargo xtask seams`, `seam:
//! client crates`): movement, the camera and combat are the runtime's, and this crate ends at the
//! action. It is this client's own device layer, not the SDK's: a bot, a script or another front
//! end drives the runtime with actions and never needs it.
//!
//! A window message (or a buffered packet from a gamepad) becomes a [`ControlCode`], is qualified
//! with the modifier bitmask and an activation bit, is resolved against a **priority-ordered stack
//! of input maps** into a numeric [`ActionId`], runs through the [`ActionState`] toggle and repeat
//! machine, and is delivered as an [`InputEvent`]. The UI takes first refusal; what it declines
//! goes to the runtime as an action ([`InputEvent::to_action`]). Two things are easily got wrong:
//!
//! 1. **Keyboard and mouse do not come from DirectInput.** Every device is acquired, but the
//!    per-frame read discards the keyboard's and mouse's buffers and decodes only joystick-class
//!    devices; everything real arrives through the window messages. DirectInput is for gamepads,
//!    and for the device *ordering* a saved keymap serialises ([`gamepad`]).
//! 2. **Input maps 1 and 2 are barriers, not maps.** They hold no bindings: [`MAP_BLOCK_KEYBOARD`]
//!    makes the walk stop for any keyboard control and [`MAP_BLOCK_ALL`] for everything. A focused
//!    text field registering the first at priority **2990** (ten below the 3000 it is handed) is
//!    the whole mechanism by which the chat bar takes the keyboard; everything it stops is at the
//!    gameplay priority (1000) or below, and it is the walk's **last** entry, which is what lets
//!    `ScrollableControls` keys reach a focused text box.

#![forbid(unsafe_code)]

/// The action ids the UI cares about, which are the shared vocabulary's.
pub use dereth_client_contract::actions::ui as action;
pub mod actionmap;
pub mod binding;
pub mod combat;
pub mod dereth;
pub mod dispatch;
pub mod error;
pub mod fire;
pub mod gamepad;
pub mod host;
pub mod keyfile;
pub mod keymap;
pub mod keys;
pub mod labels;
pub mod maps;
mod message;
pub mod mouse;
pub mod names;
pub mod objname;
pub mod pad;
pub mod presentation;
pub mod pump;
pub mod scheme;
pub mod spec;
pub mod state;
pub mod win32;

use std::path::Path;

use dereth_primitives::LocalTime;

pub use actionmap::{ActionMap, ActionMapValue, ToggleType};
pub use error::InputError;
pub use keyfile::FileNode;
pub use keymap::{InputMap, MasterInputMap};
pub use spec::{activation, ControlChord, ControlCode, DeviceType, SubControlIndex};

use dispatch::{InputMapStack, TextMode};
use fire::{ButtonHistory, ButtonHistoryEntry, ClickTiming, ControlType, RecentControlState};
use mouse::MouseState;
use state::{ActionState, ActionStateChange, ActionStates, RepeatTiming};

/// A numeric action id: the shared vocabulary's, so what this pipeline resolves a control to is
/// what the runtime's handlers match on.
pub use dereth_client_contract::actions::ActionId;

/// An input map id. Two of them are barriers rather than maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct InputMapId(pub u32);

/// Identifies one registered input-action callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CallbackId(pub u32);

/// Pseudo-map **1**: stops all **keyboard** input below this priority. A focused text control registers
/// it at [`dispatch::priority::FOCUSED_UI_TEXT_BARRIER`] (2990, *not* the focused-UI 3000 -- see
/// that constant) when it takes focus, and `DebugConsole` at 3999. Mouse controls
/// pass it,
/// which is why you can still click-to-select in the world while the chat bar has focus.
pub use maps::BLOCK_KEYBOARD as MAP_BLOCK_KEYBOARD;

/// Input map **9**, `DialogBoxes` — the pre-game screens' own map.
///
/// It carries exactly two controls in both shipped keymaps: `DIK_ESCAPE` → `EscapeKey` and
/// `DIK_RETURN` → `AcceptInput`. Unlike every other entry in [`RETAIL_MAP_REGISTRATIONS`] it has
/// **no start-up owner**: the client holds exactly three sites that register map 9 at 3000
/// and all three are *constructors* -- the three character-selection screens -- so the map is live
/// only while one of those
/// screens is up.
///
/// It has a **second, data-driven** producer that carries no literal and therefore cannot appear
/// in [`RETAIL_MAP_REGISTRATIONS`] at all: an element registers its own
/// input-map attribute, and the only two elements in the whole shipped corpus that carry
/// attribute `0x4E` name **this** map. See [`RETAIL_PER_ELEMENT_REGISTRATIONS`].
pub use maps::DIALOG_BOXES as MAP_DIALOG_BOXES;

/// The alternate-camera input map, registered at the unfocused-UI priority while the camera's
/// alternate-mode action (`0x3E`) is held.
/// It exists for keyboards without a numeric keypad: its default bindings put Rotate Camera
/// Left/Right/Up/Down on the arrow keys.
pub use maps::CAMERA_ALTERNATE as ALTERNATE_CAMERA_MAP;

/// Pseudo-map **2**: stops **everything**. Implemented in the fire path and **never registered**
/// anywhere in the retail client. Exposed because the barrier is three lines and the contract
/// should exist; nothing here registers it.
pub use maps::BLOCK_ALL as MAP_BLOCK_ALL;

/// The `InputEvent` delivered to an input-action callback or, if it declines, to the global
/// action-handler list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputEvent {
    pub action: ActionId,
    pub input_map: InputMapId,
    pub toggle: ToggleType,
    pub extent: f32,
    /// Whether this is the start of the action.
    pub start: bool,
    /// How many repeats this dispatch stands for. Greater than 1 after a stalled
    /// frame, because repeat is catch-up.
    pub repeat_delta: u32,
    /// The running repeat total.
    pub repeat_total: u32,
    /// The key-down-in-progress flag **at the moment the fire path produced this event**.
    ///
    /// Retail needs no such field: the fire path dispatches through the listener list
    /// *synchronously*, still inside the
    /// `WM_KEYDOWN`, so the wrapper reads the live flag. This build queues the event and dispatches
    /// it from the frame instead ([`InputManager::take_events`]), by which time
    /// the key-down flag is long since false — so the flag has to travel with the event or
    /// [`dispatch::TextMode::begin_dispatch`] can never see a `true` and the ignore-next-char flag
    /// can never be armed.
    ///
    /// Set it back up with [`InputManager::begin_action_dispatch`]; do not read it directly.
    pub from_key_down: bool,
}

impl InputEvent {
    /// The action this event hands on: the id, the phase and the extent, without the input map,
    /// the toggle type or the key-down context, which are this pipeline's business and not the
    /// handler's.
    ///
    /// A start with a repeat count is a repeat, any other start a begin, and a stop the end.
    #[must_use]
    pub const fn to_action(&self) -> dereth_client_contract::actions::Action {
        use dereth_client_contract::actions::{Action, ActionPhase};
        let phase = match (self.start, self.repeat_delta) {
            (false, _) => ActionPhase::End,
            (true, 0) => ActionPhase::Begin,
            (true, _) => ActionPhase::Repeat,
        };
        Action {
            id: self.action,
            phase,
            extent: self.extent,
            repeats: self.repeat_delta,
        }
    }
}

/// The Ctrl and Alt bits of `meta_mode`, by the meta keys `keymap` names: the modifiers that make
/// a key a different command, where Shift does not.
#[must_use]
pub fn command_modifiers(keymap: &MasterInputMap, meta_mode: u32) -> u32 {
    const COMMAND_KEYS: [u16; 4] = [0x1D, 0x9D, 0x38, 0xB8];
    let mask = keymap
        .meta_keys
        .iter()
        .filter(|(control, _)| COMMAND_KEYS.contains(&control.offset()))
        .fold(0, |m, (_, bit)| m | bit);
    meta_mode & mask
}

/// Action callbacks and input handlers share this shape.
/// `true` means consumed.
pub trait ActionHandler: std::fmt::Debug {
    fn on_action(&mut self, e: &InputEvent) -> bool;
}

/// Input-manager state: maps, handler stack, action states, button history, and mouse.
#[derive(Debug)]
pub struct InputManager {
    /// The merged master input map.
    pub keymap: MasterInputMap,
    /// The two **shipped** keymaps, kept so the client can rebuild the merged map without them —
    /// `(game default map 0x14000000, DefaultMap 0x14000002)`, in the order they are added.
    ///
    /// Restoring the keyboard defaults clears the keymap, then adds maps `0x10000001`
    /// and `1` **before** invoking the shared restore-default behavior. The options
    /// page reaches the
    /// same two by enum to build the *defaults* list a
    /// row is initialized with. The original client loads both from its object database; this crate has no asset
    /// source, so [`Self::init_keymap`] keeps the payloads it was already handed.
    pub shipped_maps: Option<Box<(MasterInputMap, MasterInputMap)>>,
    /// The decoded `ActionMap`, retained during keymap initialization.
    pub action_map: ActionMap,
    /// The input-map list, descending by priority.
    pub maps: InputMapStack,
    /// The action states plus the repeat sweep.
    pub actions: ActionStates,
    /// The active controls.
    active_controls: Vec<(ControlCode, RecentControlState)>,
    /// The button history.
    pub history: ButtonHistory,
    pub timing: ClickTiming,
    /// The meta-key mode.
    pub meta_key_mode: u32,
    pub mouse: MouseState,
    pub text: TextMode,
    /// Whether the main window has focus.
    pub has_focus: bool,
    /// A key pressed with Ctrl or Alt held fires only a binding that names the same Ctrl and Alt:
    /// Ctrl+C is not C. Off, a key fires the binding of the key with fewer modifiers too, as the
    /// game's own key handling does; an interface with its own key scheme turns it on.
    pub exact_command_modifiers: bool,
    /// Events produced by the last call, in dispatch order. A caller drains this instead of the
    /// client's synchronous action call, and [`dispatch::TextMode`] carries the
    /// ignore-next-char latch across the seam, so the Enter that opens the chat bar is still not typed into it.
    pub pending: Vec<InputEvent>,
    /// Whether Keystone has focus, and "is the cursor over the Keystone window", which
    /// together suppress actions 7, 8, 10 and 11.
    pub keystone_help_focused: bool,
    pub cursor_over_keystone: bool,
    /// The single pending DBCS lead byte.
    pub(crate) dbcs: win32::DbcsPairing,
    /// Characters accepted by the client, drained by
    /// [`InputManager::take_characters`]. The client hands them straight to the character handlers;
    /// buffering them is the same seam that makes [`InputManager::pending`] a queue.
    pub(crate) characters: Vec<char>,
    /// The key-hit handler — the **single, exclusive** input-handler slot
    /// the input manager fills for [`dispatch::handler_flags::KEY_HIT`].
    ///
    /// In the client this is a pointer to the options row that is capturing a binding; here it is
    /// a bool plus [`InputManager::key_hits`], because the handler lives on the other side of a
    /// seam this crate cannot reach (it needs a `UiSystem` and a `GamePlayScreen`). The behaviour
    /// that matters is not the pointer, it is what the fire path
    /// does with the answer:
    ///
    /// ```text
    /// handled = call the key-hit handler (qc);
    /// if (!handled) { ...walk the input-map list... }     // ONLY when the handler declined
    /// ```
    ///
    /// So while a binding is being captured **the map walk does not happen at all** and no action
    /// is produced — which is what stops the key you are binding also firing whatever it is
    /// currently bound to. See [`InputManager::set_key_hit_handler`].
    key_hit_handler: bool,
    /// The controls diverted to the key-hit handler since the last drain, oldest first — the same
    /// seam as [`InputManager::pending`] and for the same reason.
    key_hits: Vec<ControlChord>,
    next_callback: u32,
    /// Current input time as the input manager sees it: the `now` of the most recent
    /// [`InputManager::use_time`].
    ///
    /// The fire path stamps a *starting* action with it:
    ///
    /// ```text
    ///   if (no key already holds the action) action began = current_input_time
    /// ```
    ///
    /// The repeat sweep measures the repeat delay from that
    /// stamp. Stamping it with [`MouseState::last_input_event`] instead -- the time of the
    /// last *mouse move* -- would make a key pressed seconds after the pointer last moved already
    /// past the key-repeat delay on its first sweep: it fires a catch-up repeat on the very frame
    /// of the press, and then one per frame with no delay at all (on Backspace, **2** deletions on
    /// the press frame and 30 per second from then on).
    ///
    /// The current input time is published once per frame by the client, so within a
    /// frame every message sees the same reading; this is that reading.
    cur_time: LocalTime,
}

impl Default for InputManager {
    fn default() -> Self {
        Self::empty()
    }
}

impl InputManager {
    /// A manager with no keymap and no action map: the state the client's keymap initialisation
    /// leaves behind.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            keymap: MasterInputMap::default(),
            shipped_maps: None,
            action_map: ActionMap::default(),
            maps: InputMapStack::default(),
            actions: ActionStates::default(),
            active_controls: Vec::new(),
            history: ButtonHistory::default(),
            timing: ClickTiming::default(),
            meta_key_mode: 0,
            mouse: MouseState::default(),
            text: TextMode::default(),
            has_focus: true,
            exact_command_modifiers: false,
            pending: Vec::new(),
            keystone_help_focused: false,
            cursor_over_keystone: false,
            dbcs: win32::DbcsPairing::default(),
            characters: Vec::new(),
            key_hit_handler: false,
            key_hits: Vec::new(),
            next_callback: 1,
            cur_time: LocalTime(0.0),
        }
    }

    /// Start-up -- load the `ActionMap`
    /// (enum 1, group 8, type `0x27` → DID `0x26000000`) and the engine `DefaultMap`
    /// (keymap 1 → DID `0x14000002`) from already-read payloads.
    ///
    /// The payloads come from `AssetSource`; this crate decodes them and depends on
    /// nothing to fetch them.
    ///
    /// # Errors
    /// Any failure reported while decoding either input-map payload.
    pub fn on_startup(action_map: &[u8], default_map: &[u8]) -> Result<Self, InputError> {
        let mut m = Self::empty();
        m.action_map = ActionMap::read(action_map)?;
        m.action_map.add_dereth_actions();
        m.keymap = MasterInputMap::read(default_map)?;
        Ok(m)
    }

    /// Parse an optional user keymap over already decoded defaults, in their supplied order.
    /// Malformed user text is discarded; callers choose whether to replace their map or merge
    /// these bindings into an existing map whose header must be retained.
    #[must_use]
    pub fn load_over_defaults(
        user_file: Option<&str>,
        defaults: &[&MasterInputMap],
        actions: Option<&ActionMap>,
    ) -> MasterInputMap {
        let user = user_file.and_then(|text| MasterInputMap::from_keymap_text(text).ok());
        scheme::over_defaults(user.as_ref(), defaults, actions)
    }

    /// Step 2 of start-up: the user `.keymap` file, then the game default map
    /// (keymap `0x10000001` → DID `0x14000000`), then `DefaultMap`
    /// (keymap 1 → DID `0x14000002`).
    ///
    /// Merging **adds what is absent**, so running the user file first means a default binding is
    /// only applied to a control the user file did not mention. If the user file does not
    /// exist or fails to parse, the keymap is initialized again to clear the partial state —
    /// reproduced here by starting from an empty map.
    ///
    /// # Errors
    /// A decode failure in either dat payload. A **user-file** failure is not an error: the client
    /// clears the partial state and carries on with the defaults, and so does this.
    pub fn init_keymap(
        &mut self,
        user_file: Option<&str>,
        gm_default_map: &[u8],
        default_map: &[u8],
    ) -> Result<(), InputError> {
        let gm = MasterInputMap::read(gm_default_map)?;
        let mut dm = MasterInputMap::read(default_map)?;
        // Keep the registered client-action section in the saved map even when it has no keys.
        dm.create_input_map(dereth::INPUT_MAP);
        let merged = Self::load_over_defaults(user_file, &[&gm, &dm], Some(&self.action_map));
        self.keymap.clear();
        self.keymap.merge(&merged, true);
        self.shipped_maps = Some(Box::new((gm, dm)));
        Ok(())
    }

    /// The keyboard page clears the map and adds the two shipped maps before invoking the shared
    /// restore-default behavior:
    ///
    /// ```text
    ///   clear the keymap
    ///   add keymap 0x10000001                -> game default map
    ///   add keymap 1                         -> DefaultMap
    ///   shared restore-default behavior
    /// ```
    ///
    /// **The user's `.keymap` is deliberately not re-merged** — that is the whole point of the
    /// button. Answers `false` when [`Self::shipped_maps`] is absent (a manager built by
    /// [`Self::on_startup`] alone never saw them), so a caller can tell "restored" from "could
    /// not".
    pub fn reload_defaults(&mut self) -> bool {
        let Some(maps) = self.shipped_maps.take() else {
            return false;
        };
        self.keymap.clear();
        self.keymap.merge(&maps.0, true);
        self.keymap.merge(&maps.1, true);
        self.shipped_maps = Some(maps);
        true
    }

    /// The list the keyboard options page hands the action-keymap row as its defaults list.
    ///
    /// It fetches the game default map and `DefaultMap` and merges `DefaultMap` into the
    /// former with overwrite **false** -- note that this one merge passes
    /// **false**, which is **not** the `true` the three add-keymap sites pass -- then
    /// finds the keys for the requested action in the result.
    ///
    /// So a row's defaults are the **shipped** keys, not the player's. Filling all three of a
    /// row's lists from the merged map instead makes *Restore Defaults* on a page built after a
    /// rebind restore the rebind.
    #[must_use]
    pub fn default_keys_for_action(&self, action: ActionId, map: InputMapId) -> Vec<ControlChord> {
        if map.0 == 0 {
            return Vec::new();
        }
        let Some(maps) = self.shipped_maps.as_ref() else {
            return Vec::new();
        };
        let mut merged = maps.0.clone();
        merged.merge(&maps.1, false);
        merged
            .section(map)
            .map(|s| s.keys_for_action(action))
            .unwrap_or_default()
    }

    /// Client cleanup invokes keymap saving through the input device manager.
    ///
    /// Writes back the **full merged map**, defaults included — not just the user's overrides.
    /// A player who saved a keymap under an old client therefore keeps the *old* defaults
    /// for every action that existed then, and there is no "reset all to defaults" beyond deleting
    /// the file.
    ///
    /// # Errors
    /// [`InputError::Io`].
    pub fn save_keymap(&self, path: &Path) -> Result<(), InputError> {
        std::fs::write(path, self.keymap.to_keymap_text())?;
        Ok(())
    }

    /// Register an input map.
    pub fn register_input_map(&mut self, m: InputMapId, prio: i32, cb: CallbackId) {
        self.maps.register(m, prio, cb);
    }

    /// Unregister an input map.
    pub fn unregister_input_map(&mut self, m: InputMapId, cb: CallbackId) {
        self.maps.unregister(m, cb);
    }

    /// Unregister a callback -- every map **and** every `ActionState` the
    /// callback owns, so a destroyed panel cannot leave a held action behind.
    pub fn unregister_callback(&mut self, cb: CallbackId) {
        self.maps.unregister_callback(cb);
        self.actions.remove_by_callback(cb);
    }

    /// Allocate an id for a new input-action callback.
    pub fn new_callback(&mut self) -> CallbackId {
        let id = CallbackId(self.next_callback);
        self.next_callback += 1;
        id
    }

    /// Set text mode, which calls the switch handler and
    /// therefore sets the ignore-next-char flag when appropriate.
    pub fn set_text_mode(&mut self, on: bool) {
        self.text.set_text_mode(on);
    }

    /// Setting mouse-look mode -> the mouse-mode switch.
    ///
    /// Either way it fires the virtual device's `DIV_MOUSELOOK` control as a button press/release,
    /// which is how a keymap can bind an action to "mouse-look is on".
    pub fn set_mouse_look_mode(&mut self, on: bool, time_ms: u32) {
        self.mouse.want_mouse_look = on;
        if on {
            self.mouse.enter_mouse_look(self.has_focus);
        } else {
            self.mouse.leave_mouse_look();
        }
        if let Some(idx) = self.virtual_device_index() {
            let cs = ControlCode::new(idx, SubControlIndex::None, gamepad::DIV_MOUSELOOK_OFFSET);
            self.fire_input_event(cs, ControlType::Button, i32::from(on) * 0x80, time_ms);
        }
    }

    fn virtual_device_index(&self) -> Option<u8> {
        self.keymap
            .devices
            .iter()
            .position(|d| d.device_type == DeviceType::Virtual)
            // A keymap can never hold more than 256 devices: the index is a u8 field.
            .and_then(|i| u8::try_from(i).ok())
    }

    pub fn capture_mouse(&mut self) {
        self.mouse.add_capture();
    }

    pub fn release_mouse(&mut self) {
        self.mouse.release_capture();
    }

    /// **Authoritative**, and not `GetCursorPos()`: in mouse-look the OS cursor is warped to the
    /// screen centre every frame.
    #[must_use]
    pub const fn mouse_pos(&self) -> (i32, i32) {
        self.mouse.pos
    }

    #[must_use]
    pub fn is_action_in_progress(&self, a: ActionId) -> bool {
        self.actions.is_action_in_progress(a)
    }

    /// Resolve a control -- with the documented defaults when the control is
    /// unknown: `mode` = the current meta-mode, and `activation` = `Up` for buttons, `Analog` for
    /// axes.
    pub(crate) fn previous_control_state(
        &self,
        cs: ControlCode,
        ty: ControlType,
    ) -> RecentControlState {
        if let Some((_, s)) = self.active_controls.iter().find(|(k, _)| *k == cs) {
            return *s;
        }
        RecentControlState {
            meta_mode: self.meta_key_mode,
            activation: match ty {
                ControlType::Button => activation::UP,
                _ => activation::ANALOG,
            },
            action_matched: ActionId(0),
            input_map: InputMapId(0),
            data: if ty == ControlType::Pov { -1 } else { 0 },
        }
    }

    /// Register the key-hit handler with
    /// [`dispatch::handler_flags::KEY_HIT`] (0x20), and unregister it for `false`.
    ///
    /// The slot is **exclusive** — one key-hit handler, not a list — so this is a bool and not
    /// a stack. The client sets it when the map-warn
    /// dialog comes up and clears it on the first event it accepts;
    /// on this side the host mirrors the capturing row's own answer into it every frame, exactly
    /// as `UiShell::sync_text_mode` mirrors text mode.
    pub fn set_key_hit_handler(&mut self, on: bool) {
        if !on {
            // Anything still queued belongs to a capture that is over; the client's own
            // unregister leaves nothing behind either.
            self.key_hits.clear();
        }
        self.key_hit_handler = on;
    }

    /// Whether the key-hit handler slot is filled.
    #[must_use]
    pub fn key_hit_handler_registered(&self) -> bool {
        self.key_hit_handler
    }

    /// Drain the controls diverted to the key-hit handler, oldest first.
    pub fn take_key_hits(&mut self) -> Vec<ControlChord> {
        std::mem::take(&mut self.key_hits)
    }

    /// The fire path -- the nine steps.
    ///
    /// Returns true when the event resolved to an action.
    #[allow(clippy::too_many_lines)]
    pub fn fire_input_event(
        &mut self,
        cs: ControlCode,
        ty: ControlType,
        data: i32,
        timestamp: u32,
    ) -> bool {
        // 2. The previous state of this control, and the ActionState it matched.
        let prev = self.previous_control_state(cs, ty);

        // 3. The activation bits and the extent. Bit 31 marks this as a *live* event, which is what
        //    makes qualified-control matching asymmetric.
        let (base_activation, mut extent) = fire::di_data_to_activation_type(ty, data);
        let mut act = base_activation | activation::LIVE;
        let mut meta_mode = self.meta_key_mode;

        // 4. Button history, the double-click and tap tests, and the meta-mode update.
        if ty == ControlType::Button {
            let press = act & activation::UP == 0;
            if press {
                if let Some(h) = self.history.get(cs) {
                    if prev.activation & activation::DBL_CLICK_UP == 0
                        && timestamp.wrapping_sub(h.time) <= self.timing.double_click_ms
                    {
                        act |= activation::DBL_CLICK_DOWN;
                        let dx = (self.mouse.pos.0 - h.mouse_pos.0).abs();
                        let dy = (self.mouse.pos.1 - h.mouse_pos.1).abs();
                        if dx <= self.timing.cx_dbl_click && dy <= self.timing.cy_dbl_click {
                            act |= activation::NEARBY_DOWN;
                        }
                    }
                }
                self.history.set(
                    cs,
                    ButtonHistoryEntry {
                        time: timestamp,
                        mouse_pos: self.mouse.pos,
                    },
                );
            } else {
                // On a release the modifiers *at press time* are used, not the current ones.
                meta_mode = prev.meta_mode;
                if prev.activation & activation::DBL_CLICK_DOWN != 0 {
                    act |= activation::DBL_CLICK_UP;
                    self.history.remove(cs);
                    if prev.activation & activation::NEARBY_DOWN != 0 {
                        act |= activation::NEARBY_UP;
                    }
                }
                if let Some(h) = self.history.get(cs) {
                    if timestamp.wrapping_sub(h.time) <= self.timing.tap_ms {
                        act |= activation::TAP;
                    }
                }
            }

            // The meta-mode bit for this control, from the keymap's own meta-key table.
            let bit = self.keymap.meta_mode_from_key(cs);
            if bit != 0 {
                if press {
                    self.meta_key_mode |= bit;
                } else {
                    self.meta_key_mode &= !bit;
                }
            }
            self.history.purge(timestamp);
            if !press {
                self.active_controls.retain(|(k, _)| *k != cs);
            }
        }

        // 4b. The key-hit handler sits **between** the button
        //     history and the map walk and, when a handler is registered, replaces it: the
        //     walk over the input maps runs only when no handler consumed the event.
        let qc = ControlChord::new(cs, meta_mode, act);
        // The fire path ends in `return true;` and every early
        // exit above it returns true as well — including the `activation & 0x81` ignore and the
        // mouse-axis refusal. The **one** `return false` is the "cannot overwrite this binding"
        // arm, where a conflicting action is not
        // user-bindable; on that path the client lets the event walk the maps as well. This seam
        // cannot answer that synchronously — the verdict is computed a frame later, in
        // `GamePlayScreen::drive_key_bindings` — so it always consumes. **Declared deviation**,
        // and it is one arm of one dialog: the key-binding capture.
        let consumed_by_key_hit = self.key_hit_handler;
        if consumed_by_key_hit {
            self.key_hits.push(qc);
        }

        // 5-6. Walk the map stack — **only** when the handler declined, which is
        // the fire path's own "not handled" test. Step 7 below is *outside* that `if` in the
        // client and stays outside it here: a hold released while a capture is in flight must
        // still be released, or the repeat sweep re-fires it for ever.
        let is_keyboard = self.keymap.device_type_of(cs) == Some(DeviceType::Keyboard);
        let keymap = &self.keymap;
        let hit = if consumed_by_key_hit {
            None
        } else {
            fire::walk_input_maps(self.maps.entries(), &qc, is_keyboard, |m| keymap.section(m))
        };
        let hit = hit.filter(|h| {
            !(self.exact_command_modifiers && is_keyboard)
                || command_modifiers(keymap, qc.meta_mode)
                    == command_modifiers(keymap, h.binding.meta_mode)
        });

        // 7. A release with a live hold-family ActionState releases it.
        let releasing = ty == ControlType::Button && act & activation::UP != 0;
        if releasing && prev.action_matched.0 != 0 {
            if let Some(s) = self.actions.get(prev.action_matched) {
                if s.toggle.is_hold() {
                    self.deactivate_action_key(prev.action_matched, cs, prev.input_map);
                }
            }
        }

        // 8. Dispatch.
        let mut matched = consumed_by_key_hit;
        if let Some(hit) = hit {
            matched = true;
            match hit.action {
                ActionId(0 | 1) => {}
                fire::ACTION_POINTER_X => self.mouse.non_mouse_pointer_movement.0 += data,
                fire::ACTION_POINTER_Y => self.mouse.non_mouse_pointer_movement.1 += data,
                action => {
                    let suppressed = self.keystone_help_focused
                        || (fire::KEYSTONE_SUPPRESSED_ACTIONS.contains(&action)
                            && self.cursor_over_keystone);
                    if !suppressed {
                        // Do not turn a button release into a new activation. Keeping its
                        // zero extent lets the action-fire path deactivate it exactly once.
                        if !releasing && act & activation::DOWNISH_MASK == 0 && extent == 0.0 {
                            extent = 1.0;
                        }
                        self.fire_action_event(
                            action,
                            hit.input_map,
                            cs,
                            extent,
                            act,
                            hit.callback,
                        );
                    }
                }
            }
        }

        // 9. Remember the new state for a press or a non-button -- **whether or not an action
        // matched**. The fire path's own guard is the press flag alone, and it stores the
        // matched action (0 when nothing matched). Storing only on a match would mean an unbound
        // key never records its press, so its release would look like a repeat of the default
        // "up" state and a meta key would never clear its bit.
        if !releasing {
            let (action_matched, input_map) =
                hit.map_or((ActionId(0), InputMapId(0)), |h| (h.action, h.input_map));
            let state = RecentControlState {
                meta_mode,
                activation: act,
                action_matched,
                input_map,
                data,
            };
            match self.active_controls.iter_mut().find(|(k, _)| *k == cs) {
                Some(slot) => slot.1 = state,
                None => self.active_controls.push((cs, state)),
            }
        }
        matched
    }

    /// Fire an action event.
    fn fire_action_event(
        &mut self,
        action: ActionId,
        input_map: InputMapId,
        control: ControlCode,
        extent: f32,
        act: u32,
        callback: CallbackId,
    ) {
        let key_down = self.text.processing_key_down;
        let toggle = match self.action_map.toggle_type(input_map, action) {
            ToggleType::Invalid => ToggleType::OneShot,
            t => t,
        };
        if extent == 0.0 {
            if toggle.is_hold() {
                self.deactivate_action_key(action, control, input_map);
            }
            return;
        }
        match toggle {
            ToggleType::Hold => {
                self.activate_action_key(action, input_map, control, extent, toggle, callback);
                // No down-ish bit -> the release is issued immediately, making it a one-shot.
                if act & activation::DOWNISH_MASK == 0 {
                    self.deactivate_action_key(action, control, input_map);
                }
            }
            ToggleType::Toggle => self.toggle_action_key(action, input_map, toggle, callback),
            ToggleType::OneShot | ToggleType::Invalid => {
                // -- start = true, no state kept.
                self.pending.push(InputEvent {
                    action,
                    input_map,
                    toggle: ToggleType::OneShot,
                    extent,
                    start: true,
                    repeat_delta: 0,
                    repeat_total: 0,
                    from_key_down: key_down,
                });
            }
            ToggleType::HoldRepeat | ToggleType::HoldContinuous => {
                self.activate_action_key(action, input_map, control, extent, toggle, callback);
            }
        }
    }

    /// Activate an action key.
    fn activate_action_key(
        &mut self,
        action: ActionId,
        input_map: InputMapId,
        control: ControlCode,
        extent: f32,
        toggle: ToggleType,
        callback: CallbackId,
    ) {
        let key_down = self.text.processing_key_down;
        let existed = self.actions.get(action).is_some();
        if !existed {
            self.actions.insert(ActionState::new(
                action,
                toggle,
                Some(callback),
                // The action begins at the current input time, not the last mouse move.
                self.cur_time,
            ));
        }
        let change = self
            .actions
            .get_mut(action)
            .map_or(ActionStateChange::None, |s| {
                s.add_key_press(control, extent)
            });

        // Any of Move Forward, Move Backward or Stop Moving *starting* cancels autorun. This is the
        // input layer's half of the rule; HandleNewForwardMovement is the other
        // This matches the retail client's movement input.
        if change == ActionStateChange::Started
            && state::RUN_LOCK_CANCELLING_ACTIONS.contains(&action)
        {
            self.turn_off_run_lock(input_map);
        }
        if matches!(
            change,
            ActionStateChange::Started | ActionStateChange::Updated
        ) {
            let e = self
                .actions
                .get(action)
                .map(state::ActionState::extent)
                .unwrap_or(extent);
            self.pending.push(InputEvent {
                action,
                input_map,
                toggle,
                extent: e,
                start: true,
                repeat_delta: 0,
                repeat_total: 0,
                from_key_down: key_down,
            });
        }
    }

    /// Deactivate an action key.
    fn deactivate_action_key(
        &mut self,
        action: ActionId,
        control: ControlCode,
        input_map: InputMapId,
    ) {
        let key_down = self.text.processing_key_down;
        let Some(s) = self.actions.get_mut(action) else {
            return;
        };
        let toggle = s.toggle;
        match s.remove_key_press(control) {
            // Another key still holds the action, but the extent changed.
            ActionStateChange::Updated => {
                let e = self
                    .actions
                    .get(action)
                    .map_or(0.0, state::ActionState::extent);
                self.pending.push(InputEvent {
                    action,
                    input_map,
                    toggle,
                    extent: e,
                    start: true,
                    repeat_delta: 0,
                    repeat_total: 0,
                    from_key_down: key_down,
                });
            }
            ActionStateChange::Stopped => {
                self.actions.remove(action);
                self.pending.push(InputEvent {
                    action,
                    input_map,
                    toggle,
                    extent: 0.0,
                    start: false,
                    repeat_delta: 0,
                    repeat_total: 0,
                    from_key_down: key_down,
                });
            }
            _ => {}
        }
    }

    /// Toggle an action key.
    fn toggle_action_key(
        &mut self,
        action: ActionId,
        input_map: InputMapId,
        toggle: ToggleType,
        callback: CallbackId,
    ) {
        let key_down = self.text.processing_key_down;
        let start = if self.actions.remove(action).is_some() {
            false
        } else {
            self.actions.insert(ActionState::new(
                action,
                toggle,
                Some(callback),
                // The action begins at the current input time, not the last mouse move.
                self.cur_time,
            ));
            true
        };
        self.pending.push(InputEvent {
            action,
            input_map,
            toggle,
            extent: if start { 1.0 } else { 0.0 },
            start,
            repeat_delta: 0,
            repeat_total: 0,
            from_key_down: key_down,
        });
    }

    /// Removes the state for the movement run-lock action (value **0x30**, Autorun) and dispatches
    /// a release for it.
    fn turn_off_run_lock(&mut self, input_map: InputMapId) {
        let key_down = self.text.processing_key_down;
        if let Some(s) = self.actions.remove(state::MOVEMENT_RUN_LOCK) {
            self.pending.push(InputEvent {
                action: state::MOVEMENT_RUN_LOCK,
                input_map,
                toggle: s.toggle,
                extent: 0.0,
                start: false,
                repeat_delta: 0,
                repeat_total: 0,
                from_key_down: key_down,
            });
        }
    }

    /// On focus loss and `WM_CANCELMODE`.
    ///
    /// Every active control is removed, its meta-mode bit cleared, and a synthetic release fired,
    /// so no action stays held across a focus loss.
    pub fn release_pressed_keys(&mut self) {
        let held: Vec<(ControlCode, RecentControlState)> =
            std::mem::take(&mut self.active_controls);
        for (cs, s) in held {
            let bit = self.keymap.meta_mode_from_key(cs);
            self.meta_key_mode &= !bit;
            if s.action_matched.0 != 0 {
                self.deactivate_action_key(s.action_matched, cs, s.input_map);
            }
        }
    }

    /// The per-frame tick -- read the devices, then run the input manager's own tick.
    ///
    /// Runs once per frame from the input update's step-6 equivalent: gamepads and mouse-look
    /// deltas first, then the repeat sweep.
    pub fn use_time(&mut self, now: LocalTime) -> mouse::MouseFrameAction {
        // Publish the current input time for every key pressed before the next frame. See
        // [`Self::cur_time`].
        self.cur_time = now;
        let key_down = self.text.processing_key_down;
        let action = self.mouse.frame(now);
        if matches!(action, mouse::MouseFrameAction::Look { .. }) {
            self.mouse.recentre();
        }
        for r in self.actions.sweep(now) {
            let (toggle, input_map) = self
                .actions
                .get(r.action)
                .map_or((ToggleType::HoldRepeat, InputMapId(0)), |s| {
                    (s.toggle, InputMapId(0))
                });
            let extent = self
                .actions
                .get(r.action)
                .map_or(1.0, state::ActionState::extent);
            self.pending.push(InputEvent {
                action: r.action,
                input_map,
                toggle,
                extent,
                start: true,
                repeat_delta: r.repeat_delta,
                repeat_total: r.repeat_total,
                from_key_down: key_down,
            });
        }
        action
    }

    /// The repeat constants, sampled at construction and never refreshed.
    pub fn set_repeat_timing(&mut self, t: RepeatTiming) {
        self.actions.repeat = t;
    }

    /// Drain the events produced since the last drain.
    pub fn take_events(&mut self) -> Vec<InputEvent> {
        std::mem::take(&mut self.pending)
    }

    /// Enter the wrapper the client puts around every action-callback and input-handler walk.
    ///
    /// The wrapper is three lines and only one of them is observable:
    ///
    /// ```text
    /// processing_action_in_response_to_key_down = processing_key_down;
    /// send the action to listeners;
    /// processing_action_in_response_to_key_down = false;
    /// ```
    ///
    /// The text-mode transition consults only that latch before arming
    /// the ignore-next-char flag. Retail can read the key-down flag live because the
    /// dispatch happens inside the `WM_KEYDOWN`; this build queues the event and dispatches it from
    /// the frame, so the event's recorded key-down context is restored here.
    ///
    /// **Pair it with [`Self::end_action_dispatch`]**, and call
    /// [`Self::set_text_mode`] *between* the two if the dispatch takes a text field's focus: that
    /// bracket is the only producer of the text-mode switch mid-dispatch, and the only
    /// non-test caller of [`dispatch::TextMode::begin_dispatch`].
    pub fn begin_action_dispatch(&mut self, from_key_down: bool) {
        // The key-down flag as the fire path saw it, put back for the deferred dispatch.
        // The caller reads it from the queued event; only the plain `bool` is passed because a shell
        // whose handlers run through a message **outbox** has to re-enter the
        // same scope a second time, when it drains the messages the dispatch raised. Retail raises
        // and delivers those inside the one `WM_KEYDOWN` by broadcasting the action synchronously,
        // so both are the same key-down.
        self.text.processing_key_down = from_key_down;
        self.text.begin_dispatch();
    }

    /// Leave the wrapper at the dispatch tail. See
    /// [`Self::begin_action_dispatch`].
    pub fn end_action_dispatch(&mut self) {
        self.text.end_dispatch();
        // The borrowed key-down context goes back to what it really is outside a `WM_KEYDOWN`, so a
        // later `set_text_mode` outside any dispatch cannot arm the latch.
        self.text.processing_key_down = false;
    }
}

/// The client's input-map registration table, as data: `(owner, map, priority)`.
///
/// A rebuild's UI and gameplay layers register their own maps through
/// [`InputManager::register_input_map`]; this table is what they must reproduce, and it is here so
/// the priorities are in one place rather than scattered across three crates.
///
/// # This table cannot express every registration, and a diff against it is not a completeness check
///
/// Diffing it against `BASE_MAP_REGISTRATIONS` reports 19 absent rows, and that is expected.
/// Every row here names a **literal** map
/// id, because each was recovered from a registration call site that passes a literal. Three
/// of the client's registrations have **no literal to read**: the map id is a *field of the element
/// doing the registering* (attribute `0x4E`), so it is
/// layout data and differs per element.
///
/// ```text
///   register the element's input map at priority   // and the same for each ancestor,
///                                                   // one priority lower per generation
/// ```
///
/// Those three are the element stack's registration at 0, the activation
/// alert's at 2000 and the focus change's at 3000, and **none of them can appear as a row here**. So a census that diffs
/// the two tables and finds the remainder accounted for has *not* shown the registration set is
/// complete — it has shown the literal-id half is. They are listed in
/// [`RETAIL_PER_ELEMENT_REGISTRATIONS`] instead, so the gap is visible rather than latent.
pub const RETAIL_MAP_REGISTRATIONS: &[(&str, u32, i32)] = &[
    ("Client", 0x10, dispatch::priority::CLIENT_SYSTEM_KEYS),
    ("ui-manager", 3, dispatch::priority::LOWEST),
    ("ui-manager", 0xD, dispatch::priority::DEBUG_CONSOLE),
    ("camera", 5, dispatch::priority::GAMEPLAY),
    ("camera", 6, dispatch::priority::UNFOCUSED_UI),
    ("player", 0x1000_0007, dispatch::priority::GAMEPLAY),
    ("player", 0x1000_0008, dispatch::priority::GAMEPLAY),
    ("player", 4, dispatch::priority::GAMEPLAY),
    ("player", 0x1000_0006, dispatch::priority::GAMEPLAY),
    ("client-ui", 0x1000_0009, dispatch::priority::GAMEPLAY),
    ("client-ui", 0x1000_000C, dispatch::priority::GAMEPLAY),
    ("client-ui", 0x1000_000B, dispatch::priority::UNFOCUSED_UI),
    ("combat", 0x1000_0002, dispatch::priority::GAMEPLAY),
    ("combat", 0x1000_0003, dispatch::priority::GAMEPLAY),
    ("combat", 0x1000_0004, dispatch::priority::GAMEPLAY),
    ("combat", 0x1000_0005, dispatch::priority::GAMEPLAY),
    ("debug-console", 0xB, dispatch::priority::DEBUG_CONSOLE),
    ("debug-console", 7, dispatch::priority::DEBUG_CONSOLE),
    (
        "debug-console",
        1,
        dispatch::priority::DEBUG_CONSOLE_BARRIER,
    ),
    ("profiler", 0xC, dispatch::priority::DEBUG_CONSOLE),
    ("profiler", 3, dispatch::priority::DEBUG_CONSOLE),
    // Map 1 is the typing barrier and sits ten priority points below the focused text maps.
    ("text", 1, dispatch::priority::FOCUSED_UI_TEXT_BARRIER),
    ("text", 7, dispatch::priority::FOCUSED_UI),
    ("text", 8, dispatch::priority::FOCUSED_UI),
    ("scrollable", 0xA, dispatch::priority::FOCUSED_UI),
    (
        "main-chat",
        0x1000_000D,
        dispatch::priority::TOGGLE_CHAT_ENTRY,
    ),
    ("main-chat", 0x1000_000A, dispatch::priority::GAMEPLAY),
    ("intro", 9, dispatch::priority::FOCUSED_UI),
    ("intro", 3, dispatch::priority::FOCUSED_UI),
    ("credits", 9, dispatch::priority::FOCUSED_UI),
    ("character-management", 9, dispatch::priority::FOCUSED_UI),
];

/// **The registrations whose map id is element data rather than a literal**, which
/// [`RETAIL_MAP_REGISTRATIONS`] structurally cannot hold.
///
/// `(site, which element, priority, what it registers)`. All four use the element-registration
/// path, whose only registering statement is
/// "if the element has an input map, register it at `priority`", preceded by the parent's own
/// registration at `priority - 1` — so each site also puts every **ancestor**'s
/// input map in, one priority lower per generation. The `Rebuilt` column says where this
/// workspace makes the call.
///
/// | site | element | priority | rebuilt in |
/// |---|---|---|---|
/// | activation | the element being activated | lowest (0) | `UiShell::active_input_maps` |
/// | activation alert | the active element | unfocused UI (2000) | `UiShell::active_input_maps` |
/// | focus gain | the element gaining focus | focused UI (3000) | **not yet** — `UiShell::focused_input_maps` carries the text/scrollable overrides but not the base's own map or ancestor walk |
/// | deactivation or focus loss | the losing element | — | the unregister half of both mirrors |
///
/// **With the shipped layouts every one of these registers nothing**, as verified from code and data:
/// attribute `0x4E` is present on **2 of 2 162 elements across all 101 layouts** in
/// `client_local_English.dat`, both times as map `9` (`DialogBoxes`) and both times on a leaf edit
/// field — never on a root element, which is the only thing the first two sites can reach. The
/// client never uses `0x4E` as a literal attribute id, so the layout property is the only
/// source. The rows exist so that *"the band is empty"* and *"the band is not implemented"* stay
/// distinguishable; the third row is the one still outstanding, and it is the one a shipped
/// text-input dialog would exercise.
pub const RETAIL_PER_ELEMENT_REGISTRATIONS: &[(&str, &str, i32, &str)] = &[
    (
        "element activation",
        "the element activated",
        dispatch::priority::LOWEST,
        "its input map + each ancestor's, one lower per generation",
    ),
    (
        "the activation alert",
        "the active element",
        dispatch::priority::UNFOCUSED_UI,
        "its input map + ancestors",
    ),
    (
        "the focus change",
        "the element gaining focus",
        dispatch::priority::FOCUSED_UI,
        "its input map + ancestors, plus whatever the element's override adds",
    ),
];

/// The **29 actions** declared in the shipped `ActionMap` that have no handler and no binding.
/// They are the documented exception to "every action resolves": carried in the table, listed here
/// explicitly, and given no invented behaviour.
pub const DECLARED_UNBOUND_ACTIONS: &[u32] = &[
    0x10,
    0x11,
    0x12,
    0x13,
    0x14,
    0x15, //
    0x3F,
    0x40,
    0x41,
    0x42,
    0x43,
    0x44,
    0x45,
    0x46,
    0x47,
    0x48,
    0x49,
    0x4A,
    0x4B,
    0x4C,
    0x4D,
    0x4E,
    0x4F,
    0x50,
    0x51,
    0x52, //
    0x58,
    0x59, //
    0x1000_0001,
    0x1000_0002,
    0x1000_0004,
    0x1000_0008,
    0x1000_000A, //
    0x1000_0057,
    0x1000_0058,
    0x1000_0059,
    0x1000_0075,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The client's own map
    /// 0x10 at −1 must be last and the chat toggle at 3010 must beat the focused-UI priority.
    #[test]
    fn the_retail_registrations_order_as_documented() {
        let mut m = InputManager::empty();
        let cb = m.new_callback();
        for (_, map, prio) in RETAIL_MAP_REGISTRATIONS {
            m.register_input_map(InputMapId(*map), *prio, cb);
        }
        let e = m.maps.entries();
        assert_eq!(e.first().map(|x| x.priority), Some(4000));
        assert_eq!(e.last().map(|x| x.map), Some(InputMapId(0x10)));
        assert_eq!(e.last().map(|x| x.priority), Some(-1));
        let chat = e
            .iter()
            .position(|x| x.map == InputMapId(0x1000_000D))
            .expect("registered");
        // Two callbacks register the keyboard barrier: the debug console at 3999 and focused text at
        // the focused-UI priority **minus ten**. It is the second the chat toggle has to beat,
        // and it now walks behind the three focused maps rather than in among them.
        let text = e
            .iter()
            .position(|x| {
                x.map == MAP_BLOCK_KEYBOARD
                    && x.priority == dispatch::priority::FOCUSED_UI_TEXT_BARRIER
            })
            .expect("registered");
        assert!(chat < text, "Tab must still leave the chat bar");
    }
}
