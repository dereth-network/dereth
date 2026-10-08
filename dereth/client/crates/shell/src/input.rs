//! The client's device input: the retail input manager with the registrations the client's
//! systems make, the keymap store, and the hand-off of what it produces to the runtime as actions.
//!
//! The window forwards each message the window procedure hands the input manager, with its
//! message time: the input pipeline's tap and double-click thresholds are the **user's own OS
//! settings** and are read off `m.time_ms`, not a high-resolution clock.
//!
//! What the pipeline produces is an input event; the UI takes first refusal of those (it needs the
//! input map an event came through), and whatever it declines is handed to the runtime as an
//! action (`InputShell::hand_on`), where the movement, camera and interaction handlers take it.
//! The runtime says when the game state the registrations follow changes -- the combat mode, the
//! target mode, the alternate camera -- and `InputShell::apply_notice` is how the maps follow it.
//!
//! # The gap the message mapping once left
//!
//! The window procedure's keyboard arms forward the raw `MSG`, and
//! keyboard-event generation reads the **`lParam`**, not the `wParam`: the DirectInput
//! scan code is bits 16–23, the extended flag is bit 24, and bits 30–31 mark a Windows auto-repeat
//! the client discards. A mapping that put the virtual key in `wParam` and left `lParam` zero is
//! correct for every message the window-state machine reads and silently wrong for every
//! keyboard message the input manager reads: scan code 0 is not a key.
//! [`crate::pump::Pump::key_message`] packs the `lParam`, and `InputStats` counts a keyboard
//! message that arrives without one.

use std::path::PathBuf;

use dereth_client_runtime::actions::ActionQueue;
use dereth_client_runtime::shell::ControlNotice;
use dereth_input::{ActionId, CallbackId, InputEvent, InputManager, InputMapId};
use dereth_primitives::LocalTime;

pub mod horizon_scheme;
mod keymap_files;
mod registrations;

pub use keymap_files::{
    keymap_path_for, save_keymap_preference, scheme_file, scheme_name, ClassicKeymap, SaveKeymapAs,
    CLASSIC_KEYMAP_FILE, CLASSIC_SLUG, DEFAULT_KEYMAP_FILE, MODERN_SLUG,
};
pub use registrations::{
    focused_map_priority, BASE_MAP_REGISTRATIONS, FOCUSED_TEXT_MAPS,
    FOCUSED_TEXT_MAP_REGISTRATIONS, SCROLLABLE_INPUT_MAP, TARGET_INPUT_MAP, WHOLE_RUN_INPUT_MAPS,
};

/// Anything that stops the input pipeline from coming up.
///
/// Keymap initialization failing is not survivable: with no `ActionMap` no control
/// resolves to anything and the client is deaf.
#[derive(Debug, thiserror::Error)]
pub enum InputShellError {
    /// The default action-map lookup `(1, 8, 0x27)` or a key-map merge could not resolve its object.
    #[error("the input tables are unavailable: {0}")]
    Tables(String),
    /// A payload that resolved and would not decode.
    #[error("{0}")]
    Decode(#[from] dereth_input::InputError),
}

/// Counters for the two things this seam tolerates.
///
/// The standing rule: a tolerated failure gets a counter and the counter gets asserted on.
/// A `MSG` that does not forward is **not** a failure — that is the table doing
/// its job — but a forwarded keyboard message whose `lParam` carries no scan code is exactly the
/// bug described in the module documentation, so it has a number of its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputStats {
    /// Messages handed to `InputManager::on_window_event`.
    pub messages_offered: u64,
    /// Messages it consumed (the window procedure forwards, the message handler handles).
    pub messages_handled: u64,
    /// Keyboard messages whose `lParam` decoded to no scan code: either a Windows auto-repeat,
    /// which the client deliberately discards, or a `MSG` the pump built wrongly.
    pub keyboard_without_scan_code: u64,
    /// `InputEvent`s produced.
    pub actions_fired: u64,
    /// Characters accepted by the input manager's character update.
    pub characters: u64,
}

/// The input manager plus the registrations made by UI initialization and the player system.
#[derive(Debug)]
pub struct InputShell {
    /// The input manager. Public because it *is* the API; this module adds nothing to it.
    pub manager: InputManager,
    /// The callback id used during UI initialization to register the base maps.
    client: CallbackId,
    /// The **focused** text element's action callback, under which its input
    /// maps live. Separate from [`Self::client`] so that dropping it releases the actions it holds.
    /// See `InputShell::set_focused_text_maps`.
    text_callback: CallbackId,
    /// The **active root element**'s input-action callback, used for the two activation-time
    /// registrations at priorities 0 and 2000.
    /// See [`InputShell::set_active_input_maps`].
    ///
    /// A third id rather than [`Self::text_callback`] because they are two different elements in
    /// the client — the active *window* and the focused *descendant* — with two different
    /// lifetimes, and `unregister_callback` is per callback.
    active_callback: CallbackId,
    /// The **current pre-game screen**'s input-action callback — the receiver used by the intro,
    /// credits and character-management screen-lifetime registrations. See
    /// [`InputShell::set_mode_input_maps`].
    ///
    /// A fifth id rather than [`Self::client`] because in the client it is a different object with
    /// a different lifetime: the maps go in with the screen's constructor and out with its
    /// destructor, and the screen — not the element manager — is the callback
    /// listener dispatch gives first refusal to. That is the premise of the UI shell's
    /// mode-action handler.
    mode_callback: CallbackId,
    /// The combat subsystem's own input-action callback — the embedded receiver that
    /// character-session start and the element map registration both pass.
    /// See [`InputShell::register_combat_input_maps`].
    ///
    /// A fourth id rather than [`Self::client`] because the combat map registration's middle step re-registers
    /// `0x10000002` on every mode change and that step is a **no-op** in the client: map registration
    /// rejects an exact `{map, callback, priority}` triple, and the triple already
    /// exists *under this callback*. Registered under the client's id instead, the same call would
    /// insert a second entry and walk `Combat` twice — and the mode map would no longer be the
    /// first `priority::GAMEPLAY` map walked, which is the whole point of the ordering.
    combat_callback: CallbackId,
    /// The UI subsystem's target-mode callback, separate from unrelated base/held actions.
    ///
    /// Public so a test that drives the shell through `crate::pump::Pump` can read it; a callback
    /// id beside the already-public [`Self::manager`] widens nothing.
    pub target_callback: CallbackId,
    target_input_map_active: bool,
    /// The callback the character-session maps other than combat's are registered under — every
    /// row of [`BASE_MAP_REGISTRATIONS`] outside [`WHOLE_RUN_INPUT_MAPS`] whose owner is not the
    /// combat subsystem. See [`InputShell::set_character_session_input_maps`].
    ///
    /// Its own id rather than [`Self::client`] because ending the session drops the whole
    /// callback. Held actions are released and handed on first, before unregistration discards
    /// their state: a movement key held through log-off must not stay held on the character screen.
    session_callback: CallbackId,
    /// Whether the character-session maps are registered. See
    /// [`InputShell::set_character_session_input_maps`].
    character_session_input_maps: bool,
    /// The combat mode reflected in the **registrations** — the mode
    /// [`InputShell::register_combat_input_maps`] was last given as its `new`.
    ///
    /// Its constructor value is `NONCOMBAT_COMBAT_MODE` (1), which is what
    /// the combat-system constructor writes (combat mode `NONCOMBAT_COMBAT_MODE`, pending
    /// combat mode `UNDEF_COMBAT_MODE`) and what `dereth_client_model::CombatState::default`
    /// agrees on — so a fresh shell and a fresh world are in the same state and the first poll
    /// finds nothing to do.
    combat_input_mode: u32,
    pub stats: InputStats,
    /// The events the last frame produced, drained by the application.
    events: Vec<InputEvent>,
    runtime_pending: Vec<dereth_client_contract::actions::Action>,
    /// Actions synthesised by a host or a test since the last frame, spliced into
    /// [`Self::events`] by [`Self::use_time`] *after* the frame's expiry sweep.
    ///
    /// It exists because the sweep and a synthetic action are otherwise indistinguishable.
    /// Injecting through [`Self::put_back_unconsumed`] does not work: its contract is "here is
    /// what **this frame's** stage declined", and since an event that nobody claims is not
    /// carried to the next frame, an injection made *between* frames would be swept away before
    /// it is ever offered. This is the injection seam, and it is separate on purpose.
    injected: Vec<InputEvent>,
    /// What the input poll's step 3 decided about the pointer this frame, kept for the one
    /// input-handler slot the UI occupies. Input simulation returns it, and the UI shell takes it
    /// from here.
    mouse_frame: dereth_input::mouse::MouseFrameAction,
    /// A `WM_MOUSELEAVE` arrived: its message row calls the leave handler directly, and the
    /// input manager cannot reach the UI, so the message is recorded here and `dereth_client_shell::ui::UiShell` makes the call.
    mouse_left_window: bool,
    /// The keymap filename, resolved to a full path.
    ///
    /// Keymap initialization computes the path once; cleanup writes the merged map back to it.
    /// `None` here is the client's keymap-file path being empty, in which
    /// case cleanup skips the save — the only state in which a rebind does not persist.
    keymap_path: Option<PathBuf>,
    /// The classic interface's key map, once it is wanted.
    classic: Option<ClassicKeymap>,
    modern: dereth_input::MasterInputMap,
    classic_active: bool,
    /// The Horizon interface's key map, once it is wanted, and whether it is the one in use.
    horizon: Option<horizon_scheme::HorizonKeymap>,
    horizon_active: bool,
    focused_maps: Vec<(u32, i32)>,
    mode_maps: Vec<u32>,
    retiring_session: bool,
    retiring_actions: Vec<dereth_client_contract::actions::Action>,
    /// The characters the input manager accepted this frame, kept for the text element's
    /// character handler.
    ///
    /// The client hands each to its input-handler list, and the only handler that wants
    /// characters is the **focused** text element.
    characters: Vec<char>,
}

impl InputShell {
    /// Deliver the old scope's releases to runtime before callback unregistration drops state.
    pub fn finish_session_retirement(
        &mut self,
        receive: impl FnOnce(Vec<dereth_client_contract::actions::Action>),
    ) {
        if !self.retiring_session {
            return;
        }
        let mut released = std::mem::take(&mut self.runtime_pending);
        released.extend(self.take_events().iter().map(InputEvent::to_action));
        released.append(&mut self.retiring_actions);
        receive(released);
        self.set_target_input_map(false);
        self.manager.unregister_callback(self.session_callback);
        self.manager.unregister_callback(self.combat_callback);
        self.combat_input_mode = dereth_input::combat::mode::NONCOMBAT;
        self.retiring_session = false;
        if self.character_session_input_maps {
            self.character_session_input_maps = false;
            self.set_character_session_input_maps(true);
        }
    }

    fn modern_map(&self) -> &dereth_input::MasterInputMap {
        if self.classic_active || self.horizon_active {
            &self.modern
        } else {
            &self.manager.keymap
        }
    }

    fn modern_map_mut(&mut self) -> &mut dereth_input::MasterInputMap {
        if self.classic_active || self.horizon_active {
            &mut self.modern
        } else {
            &mut self.manager.keymap
        }
    }

    /// Release while the old callback scope still exists. The caller must deliver these
    /// actions before retiring that scope or replacing the active map.
    pub fn release_actions(&mut self) -> Vec<dereth_client_contract::actions::Action> {
        self.manager.release_pressed_keys();
        self.manager.meta_key_mode = 0;
        self.manager.set_key_hit_handler(false);
        self.manager.take_key_hits();
        self.events.extend(self.manager.take_events());
        let mut actions = std::mem::take(&mut self.runtime_pending);
        actions.extend(self.take_events().iter().map(InputEvent::to_action));
        actions
    }

    /// Select a complete saved map after the outgoing actions have been delivered.
    pub fn activate_classic(&mut self, classic: bool) {
        if classic == self.classic_active {
            return;
        }
        let next = if classic {
            self.modern = self.manager.keymap.clone();
            self.classic_keymap().map.clone()
        } else {
            self.classic.as_mut().expect("active classic map").map = self.manager.keymap.clone();
            self.modern.clone()
        };
        self.set_focused_input_maps(&[]);
        self.set_active_input_maps(&[]);
        self.set_mode_input_maps(&[]);
        self.manager.set_text_mode(false);
        self.manager.keymap = next;
        self.classic_active = classic;
    }

    /// A bindable chat recall command remains available while this interface owns text focus.
    pub fn classic_recall_scope(&mut self, active: bool) {
        let map = dereth_input::dereth::INPUT_MAP;
        if active {
            self.manager.maps.register_scoped(
                map,
                3001,
                self.text_callback,
                Some(dereth_client_contract::actions::dereth::REPEAT_LAST_MESSAGE),
            );
        } else {
            self.manager.unregister_input_map(map, self.text_callback);
        }
    }

    /// Input-manager startup precedes gameplay-client keymap initialization.
    ///
    /// Both dat objects are resolved through [`dereth_client_runtime::assets::enum_did`], never written down:
    /// the `ActionMap` is group 8 entry 1 and the two key maps are group 10 entries 1 and
    /// `0x10000001`.
    ///
    /// The **order** of keymap initialization matters: the user's `.keymap` file
    /// first, then the game keymap, then the base keymap, because merging *adds what is absent*, so
    /// a default binding only reaches a control the user file did not mention. A user file that
    /// will not parse is not an error — the client clears the partial state and carries on.
    ///
    /// # Errors
    /// [`InputShellError`] when either table is missing or will not decode.
    pub fn new(
        assets: &dyn dereth_primitives::AssetSource,
        user_keymap: Option<&PathBuf>,
    ) -> Result<Self, InputShellError> {
        use dereth_client_runtime::assets::enum_did;

        let missing = |what: &str| InputShellError::Tables(format!("{what} did not resolve"));
        let action_map_did =
            enum_did(assets, 8, 1).ok_or_else(|| missing("ActionMap (group 8)"))?;
        let default_did =
            enum_did(assets, 10, 1).ok_or_else(|| missing("DefaultMap (group 10)"))?;
        let gm_default_did =
            enum_did(assets, 10, 0x1000_0001).ok_or_else(|| missing("game keymap (group 10)"))?;

        let read = |d| {
            assets
                .read(d)
                .map_err(|e| InputShellError::Tables(format!("{d:?}: {e}")))
        };
        let action_map = read(action_map_did)?;
        let default_map = read(default_did)?;
        let gm_default_map = read(gm_default_did)?;

        let mut manager = InputManager::on_startup(&action_map, &default_map)?;
        // The user's file is read as text; an absent one is the ordinary case.
        let user = user_keymap
            .and_then(|p| dereth_client_runtime::platform::files::read_to_string(p).ok());
        manager.init_keymap(user.as_deref(), &gm_default_map, &default_map)?;

        let client = manager.new_callback();
        // the combat subsystem's own callback. The combat rows go in under it,
        // because session startup re-registers `0x10000002` under the *same* callback it already
        // used, which is what makes that step inert.
        let combat_callback = manager.new_callback();
        let session_callback = manager.new_callback();
        // A bare shell starts with the character session's maps up, so a client with no UI — and
        // therefore no screen to say it is not in the world — keeps every gameplay key. The UI
        // takes them away on its first frame on a pre-game screen.
        for (owner, map, prio) in BASE_MAP_REGISTRATIONS {
            let cb = if WHOLE_RUN_INPUT_MAPS.contains(map) {
                client
            } else if *owner == "combat" {
                combat_callback
            } else {
                session_callback
            };
            manager.register_input_map(InputMapId(*map), *prio, cb);
        }

        // The focused text element's action callback, distinct from the client's so that
        // dropping it takes its held actions with it. See [`Self::set_focused_text_maps`].
        let text_callback = manager.new_callback();
        // The active root element's own callback. See [`Self::set_active_input_maps`].
        let active_callback = manager.new_callback();
        // The current pre-game screen's own callback. See [`Self::set_mode_input_maps`].
        let mode_callback = manager.new_callback();
        let target_callback = manager.new_callback();
        let modern = manager.keymap.clone();
        Ok(Self {
            manager,
            client,
            text_callback,
            active_callback,
            mode_callback,
            combat_callback,
            target_callback,
            target_input_map_active: false,
            session_callback,
            character_session_input_maps: true,
            combat_input_mode: dereth_input::combat::mode::NONCOMBAT,
            stats: InputStats::default(),
            events: Vec::new(),
            runtime_pending: Vec::new(),
            injected: Vec::new(),
            mouse_frame: dereth_input::mouse::MouseFrameAction::None,
            mouse_left_window: false,
            keymap_path: user_keymap.cloned(),
            classic: None,
            modern,
            classic_active: false,
            horizon: None,
            horizon_active: false,
            focused_maps: Vec::new(),
            mode_maps: Vec::new(),
            retiring_session: false,
            retiring_actions: Vec::new(),
            characters: Vec::new(),
        })
    }

    // ---------------------------------------------------------------------------------------
    // Rebinding a key, and making it survive the session
    // ---------------------------------------------------------------------------------------

    // ---------------------------------------------------------------------------------------
    // The classic interface's key map
    // ---------------------------------------------------------------------------------------

    /// Offer one captured control and get the
    /// client's own verdict. See [`dereth_input::binding::Capture`].
    ///
    /// This is the whole reason the back end is reachable without the options page: the page owns
    /// the modal dialog and the row of key buttons, and **nothing else** in the key-binding option
    /// row is policy. A test — or the page — drives these two calls.
    #[must_use]
    pub fn capture_key_hit(
        &self,
        map: InputMapId,
        action: ActionId,
        control: dereth_input::ControlChord,
        confirmed: bool,
    ) -> dereth_input::binding::Capture {
        self.manager
            .capture_key_hit(map, action, control, confirmed)
    }

    /// Applying an action-key-map option sets one binding.
    ///
    /// **It takes effect immediately and by construction**: `fire::walk_input_maps` reads
    /// `manager.keymap.sections` on the very next `WM_KEYDOWN`, so there is no cache to invalidate
    /// and no "apply" step. The client's own epilogue sends an action-key-mapping
    /// refresh notice, which repaints the options row and nothing else.
    pub fn set_binding(
        &mut self,
        map: InputMapId,
        action: ActionId,
        slot: Option<usize>,
        control: dereth_input::ControlChord,
    ) -> bool {
        self.manager.set_binding(map, action, slot, control)
    }

    /// The input manager's find-keys-for-action query — the controls an options row shows for one
    /// action.
    #[must_use]
    pub fn keys_for_action(
        &self,
        action: ActionId,
        map: InputMapId,
    ) -> Vec<dereth_input::ControlChord> {
        self.manager.find_keys_for_action(action, map)
    }

    /// Register an input handler with
    /// [`dereth_input::dispatch::handler_flags::KEY_HIT`], and unregister it for `false`.
    ///
    /// The client's handler is the option row currently capturing a binding, registered when the
    /// map-warning dialog appears. This is the producer for the page's `key_bindings_key_hit`
    /// drain; see `dereth_client::app::App::drive_key_bindings`, which is the mirror that fills
    /// it.
    ///
    /// While it is filled, `fire_input_event` walks no input map, so the key being bound
    /// does not also fire whatever it is currently bound to.
    pub fn set_key_hit_handler(&mut self, on: bool) {
        self.manager.set_key_hit_handler(on);
    }

    /// The controls diverted to the key-hit handler since the last drain, oldest first.
    pub fn take_key_hits(&mut self) -> Vec<dereth_input::ControlChord> {
        self.manager.take_key_hits()
    }

    /// Process one `MSG`, with its `GetMessageTime()`.
    ///
    /// Returns whether the input manager consumed it.
    pub fn on_message(&mut self, m: dereth_input::win32::Win32Message) -> bool {
        self.stats.messages_offered += 1;

        // The auto-repeat / no-scan-code case, counted before the manager sees it so the number
        // means "the pump built a keyboard MSG the pipeline could not read", which is the bug the
        // module documentation describes.
        if matches!(
            m.message,
            dereth_input::win32::msg::WM_KEYDOWN
                | dereth_input::win32::msg::WM_KEYUP
                | dereth_input::win32::msg::WM_SYSKEYDOWN
                | dereth_input::win32::msg::WM_SYSKEYUP
        ) && dereth_input::win32::keyboard_offset(m.lparam).is_none()
        {
            self.stats.keyboard_without_scan_code += 1;
        }

        // `IsDBCSLeadByte` is a function of the process code page. This build runs the English
        // client, whose ANSI code page is single-byte throughout, so nothing is ever a lead byte;
        // the hook is injected rather than assumed because a Japanese or Korean install differs.
        // The message handler forwards `WM_MOUSELEAVE` to the UI element manager's mouse-leave event.
        // The input manager returns "handled" and has no UI to call, so the fact is recorded for the shell.
        if m.message == dereth_input::win32::msg::WM_MOUSELEAVE {
            self.mouse_left_window = true;
        }
        let handled = self.manager.on_window_event(&m, &|_| false);
        if handled {
            self.stats.messages_handled += 1;
        }
        handled
    }

    /// Make a normalized message available to the active interface immediately.
    pub fn collect_message(&mut self) {
        let (events, chars) = self.drain_message();
        self.events.extend(events);
        self.characters.extend(chars);
    }

    /// Drain one normalized message before the next one changes focus or text mode.
    pub fn drain_message(&mut self) -> (Vec<InputEvent>, Vec<char>) {
        let events = self.manager.take_events();
        let chars = self.manager.take_characters();
        self.stats.actions_fired += events.len() as u64;
        self.stats.characters += chars.len() as u64;
        (events, chars)
    }

    /// Input-manager simulation — step 6 of the UI-element update: the gamepad
    /// poll, the mouse-look delta and the repeat sweep, then drain what stage 3 produced.
    pub fn use_time(&mut self, now: LocalTime) {
        // Step 3 of the input poll: the pointer moved, or it did not. The answer is kept,
        // because the UI element manager is registered in the mouse-move list and this is the only place
        // the call would come from.
        self.mouse_frame = self.manager.use_time(now);
        // An input event is dispatched for exactly one frame, then it is gone: the UI takes first
        // refusal and [`Self::hand_on`] gives the rest to the runtime's action queue in the same
        // frame, which drops whatever no stage claimed. Nothing is carried here from one frame to
        // the next.
        // ...and anything a host or a test synthesised since the last frame, which stands in for
        // `fire_input_event` and must therefore arrive with this frame's events rather than be
        // lost. See [`Self::inject_action`].
        let injected = std::mem::take(&mut self.injected);
        self.stats.actions_fired += injected.len() as u64;
        self.events.extend(injected);
        let fired = self.manager.take_events();
        self.stats.actions_fired += fired.len() as u64;
        self.events.extend(fired);
        let chars = self.manager.take_characters();
        self.stats.characters += chars.len() as u64;
        self.characters.extend(chars);
    }

    /// The characters accepted since the last drain, oldest first.
    ///
    /// Character updating has already applied the three gates
    /// (the ignore-next-character latch, text mode, main-window focus), so everything here is a
    /// character the client would deliver.
    pub fn take_characters(&mut self) -> Vec<char> {
        std::mem::take(&mut self.characters)
    }

    /// Set text mode.
    ///
    /// In the client this is called by the text element's message 0x2F
    /// arm; `UiSystem` has no route to the input manager, so `dereth_client_shell::ui::UiShell::frame`
    /// mirrors the focus element into it instead. The ignore-next-character latch inside is what
    /// stops the key that opened chat from becoming the line's first character.
    pub fn set_text_mode(&mut self, on: bool) {
        self.manager.set_text_mode(on);
    }

    /// The actions produced since the last drain, in dispatch order.
    pub fn take_events(&mut self) -> Vec<InputEvent> {
        std::mem::take(&mut self.events)
    }

    /// What the input poll's step 3 decided about the pointer, taken once.
    pub fn take_mouse_frame(&mut self) -> dereth_input::mouse::MouseFrameAction {
        std::mem::replace(
            &mut self.mouse_frame,
            dereth_input::mouse::MouseFrameAction::None,
        )
    }

    /// The actions declined, put back for whoever else is listening.
    ///
    /// Sending an action to listeners:
    /// if the action has a callback and it consumes the action, stop; otherwise offer it to each
    /// input handler — the winning map's callback gets first refusal and what it consumes reaches
    /// nobody else. The UI is that callback for input map 3, so it takes the whole queue, keeps
    /// what it acted on and returns the rest here.
    ///
    /// Synthesise one action, matching input-event firing when a control resolves through the map
    /// walk.
    ///
    /// The event is queued for the **next** [`Self::use_time`], which splices it in after that
    /// frame's expiry sweep, so it is dispatched exactly once and then expires like any other.
    ///
    /// Its callers are tests that need an action with a chosen `input_map` and no device behind
    /// it, such as the intro screen's own maps. [`Self::put_back_unconsumed`] is a different
    /// contract - "what this frame's stage declined" - and an unclaimed event is not carried
    /// across frames, so it cannot inject.
    pub fn inject_action(&mut self, e: InputEvent) {
        self.injected.push(e);
    }

    pub fn put_back_unconsumed(&mut self, mut events: Vec<InputEvent>) {
        // The queue is normally empty at this point; anything that arrived meanwhile is newer and
        // must stay newer.
        events.append(&mut self.events);
        self.events = events;
    }

    /// A `WM_MOUSELEAVE` has arrived since the last call.
    pub fn take_mouse_left_window(&mut self) -> bool {
        std::mem::take(&mut self.mouse_left_window)
    }

    /// The input manager's mouse X/Y — its stored mouse position, which is **authoritative**:
    /// in mouse-look the OS cursor is warped to the screen centre every frame, so the UI hit-tests
    /// against this and never against `GetCursorPos()`.
    #[must_use]
    pub fn mouse_pos(&self) -> (i32, i32) {
        self.manager.mouse_pos()
    }

    /// Whether an action is currently held.
    #[must_use]
    pub fn is_action_in_progress(&self, a: ActionId) -> bool {
        self.manager.is_action_in_progress(a)
    }

    /// Is a keyboard barrier registered right now — i.e. would
    /// `fire::walk_input_maps` `break` before any gameplay map for a control on the keyboard
    /// device?
    ///
    /// This is the *state* the barrier is in, read off the live map stack rather than
    /// re-derived: [`dereth_input::MAP_BLOCK_KEYBOARD`] (map 1) breaks the walk for a keyboard
    /// control and [`dereth_input::MAP_BLOCK_ALL`] (map 2) breaks it for everything, so either one
    /// being registered means no keyboard control below it can reach a gameplay map.
    ///
    /// Nothing in the client asks this question — the client does not need to, because *all* of
    /// its keyboard input goes through the walk. It exists here for the one control that has no
    /// action behind it: the debug flycam's vertical, which is a rebuild scaffold with no binding
    /// in the shipped keymap and therefore cannot be gated by the walk. See
    /// `dereth_client::app::App::note_flycam_input`.
    #[must_use]
    pub fn keyboard_blocked(&self) -> bool {
        self.manager.maps.entries().iter().any(|e| {
            e.map == dereth_input::MAP_BLOCK_KEYBOARD || e.map == dereth_input::MAP_BLOCK_ALL
        })
    }
}

/// The input-pump step called by the UI manager's per-frame update.
///
/// `dereth_ui::UiSystem::use_time` calls `InputPump::use_time` at exactly the point the client calls
/// the input manager's per-frame update, so the two crates meet here and nowhere else.
impl dereth_client_contract::InputPump for InputShell {
    fn use_time(&mut self, now: LocalTime) {
        InputShell::use_time(self, now);
    }

    /// Test the Shift meta-key bit using the keymap's mapping for the left Shift scan code.
    ///
    /// This is the real implementation of [`dereth_client_contract::InputPump::shift_key_down`],
    /// whose default is `false`. It is latched once a frame into `UiSystem::shift_key_down` and
    /// read by `mouse_move_element`, `mouse_resize_element` and text selection's shift-extends
    /// arm; without it the ten-pixel grid snap on a dragged or resized window never fires.
    ///
    /// `DIK_RSHIFT` is folded onto `DIK_LSHIFT` by [`dereth_input::win32`] before the map sees it,
    /// so both shift keys set the bit; `dereth-input`'s `right_shift_sets_the_shift_bit` pins that.
    /// The mask comes from the **merged keymap's own** meta-key table rather than a constant,
    /// which is what `meta_mode_from_key` does and is why a rebound shift would still work.
    fn shift_key_down(&self) -> bool {
        /// `DIK_LSHIFT`, keyboard device 0, button sub-control — `names.rs`'s own table.
        const DIK_LSHIFT: u16 = 0x2A;
        let cs = dereth_input::spec::ControlCode::new(
            0,
            dereth_input::spec::SubControlIndex::None,
            DIK_LSHIFT,
        );
        let mask = self.manager.keymap.meta_mode_from_key(cs);
        mask != 0 && self.manager.meta_key_mode & mask != 0
    }
}

impl InputShell {
    /// Hand this frame's input events on to the runtime as actions: what the UI declined, for the
    /// movement, camera and interaction stages. The queue here is empty afterwards.
    pub fn hand_on(&mut self, actions: &mut ActionQueue) {
        self.finish_session_retirement(|released| actions.submit(released));
        actions.submit(std::mem::take(&mut self.runtime_pending));
        let events = self.take_events();
        actions.submit(events.iter().map(InputEvent::to_action));
    }

    /// Keep UI-declined host actions separate from the next message's UI candidates.
    pub fn defer_runtime_actions(
        &mut self,
        actions: impl IntoIterator<Item = dereth_client_contract::actions::Action>,
    ) {
        self.runtime_pending.extend(actions);
    }

    /// Finish one message's first-refusal walk without re-offering it on another message.
    pub fn defer_declined_actions(&mut self) {
        let events = self.take_events();
        self.defer_runtime_actions(events.iter().map(InputEvent::to_action));
    }

    /// Follow a change in the game state the registrations depend on.
    ///
    /// * the combat mode swaps the one live combat-mode map ([`Self::set_combat_input_maps`]);
    /// * a target mode registers the targeting map at priority 2000 ([`Self::set_target_input_map`]);
    /// * holding the camera's alternate-mode action registers the alternate camera map at
    ///   `priority::UNFOCUSED_UI`, and releasing it unregisters it -- which is why the arrow keys
    ///   rotate the camera only while it is held, instead of walking.
    pub fn apply_notice(&mut self, notice: ControlNotice) {
        match notice {
            ControlNotice::CombatMode(mode) => {
                self.set_combat_input_maps(mode);
            }
            ControlNotice::TargetMode(active) => self.set_target_input_map(active),
            ControlNotice::AlternateCamera(on) => {
                let on = on && !self.classic_active;
                let map = dereth_input::ALTERNATE_CAMERA_MAP;
                if on {
                    self.manager.register_input_map(
                        map,
                        dereth_input::dispatch::priority::UNFOCUSED_UI,
                        self.client,
                    );
                } else {
                    self.manager.unregister_input_map(map, self.client);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_client_runtime::actions::movement::action;
    use dereth_input::keys::Key;
    use dereth_input::win32::msg;
    use dereth_input::{InputManager, InputMapId};

    /// The keymap file hangs off the preferences file's directory, like everything else the client
    /// saves there: named by the preference, defaulted to a constant, and nothing at all when there
    /// is no preferences file.
    ///
    /// Client divergence CD-007: the default is `dereth-modern.keymap`, where retail's is named after the
    /// running program.
    ///
    /// Behaviour: presentation.settings.the-default-key-map-is-named-for-this-client
    #[test]
    fn the_keymap_file_hangs_off_the_preferences_files_directory() {
        let prefs = std::path::PathBuf::from("root/dir")
            .join(dereth_client_runtime::config::PREFERENCES_FILE_NAME);
        let dir = prefs.parent().expect("a parent");

        // The keymap path, named and defaulted during input initialization.
        let keymap = keymap_path_for(&prefs, Some("chosen.keymap"))
            .expect("a non-empty preferences path has a keymap path");
        assert_eq!(keymap, dir.join("chosen.keymap"));
        let defaulted =
            keymap_path_for(&prefs, None).expect("a non-empty preferences path has a keymap path");
        assert_eq!(
            defaulted,
            dir.join(DEFAULT_KEYMAP_FILE),
            "the default is a constant, not this executable's name"
        );
        assert_eq!(DEFAULT_KEYMAP_FILE, "dereth-modern.keymap");
        // An empty preference is the same as no preference, which is retail's own test.
        assert_eq!(keymap_path_for(&prefs, Some("")), Some(defaulted));
        // An empty preferences path is the one state that writes nothing at all.
        assert_eq!(keymap_path_for(&std::path::PathBuf::new(), None), None);
    }

    #[test]
    fn keymap_preference_merge_preserves_valid_neighbors_and_refuses_invalid_bytes() {
        let dir =
            std::env::temp_dir().join(format!("dereth-keymap-preferences-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("stale disposable directory clears");
        }
        std::fs::create_dir_all(&dir).expect("disposable preferences directory");

        let valid = dir.join("valid.ini");
        std::fs::write(&valid, "[Net]\r\nUserName=keep-me\r\n")
            .expect("valid disposable preferences");
        save_keymap_preference(&valid, "chosen.keymap").expect("the merge succeeds");
        let merged = std::fs::read_to_string(&valid).expect("merged preferences remain text");
        assert!(
            merged.contains("UserName=keep-me"),
            "an unrelated preference survives: {merged}"
        );
        assert!(
            merged.contains("[Input]\r\nKeymapFile=chosen.keymap"),
            "the new section is added"
        );

        let invalid = dir.join("invalid.ini");
        let bytes = [0xFF, 0xFE, 0x00, 0x81];
        std::fs::write(&invalid, bytes).expect("invalid disposable preferences");
        assert!(save_keymap_preference(&invalid, "must-not-write.keymap").is_err());
        assert_eq!(
            std::fs::read(&invalid).expect("invalid file still exists"),
            bytes
        );

        std::fs::remove_dir_all(&dir).expect("disposable preferences directory clears");
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn save_keymap_as_classifies_existing_and_read_only_targets_before_writing() {
        let dir =
            std::env::temp_dir().join(format!("dereth-keymap-save-as-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("stale disposable directory clears");
        }
        std::fs::create_dir_all(&dir).expect("disposable keymap directory");
        let current = dir.join("current.keymap");
        let mut shell = InputShell::new(&store(), Some(&current)).expect("required shipped maps");

        let existing = dir.join("existing-modern.keymap");
        std::fs::write(&existing, b"sentinel").expect("existing target");
        assert_eq!(
            shell
                .save_keymap_as("existing", false)
                .expect("classification"),
            Some(SaveKeymapAs::NeedsOverwrite)
        );
        assert_eq!(
            std::fs::read(&existing).expect("existing bytes"),
            b"sentinel"
        );
        assert_eq!(
            shell
                .save_keymap_as("existing", true)
                .expect("confirmed write"),
            Some(SaveKeymapAs::Saved)
        );
        assert_ne!(
            std::fs::read(&existing).expect("overwritten bytes"),
            b"sentinel"
        );

        let read_only = dir.join("read-only-modern.keymap");
        std::fs::write(&read_only, b"read-only sentinel").expect("read-only target");
        let mut permissions = std::fs::metadata(&read_only)
            .expect("metadata")
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&read_only, permissions).expect("mark target read-only");
        assert_eq!(
            shell
                .save_keymap_as("read-only", false)
                .expect("classification"),
            Some(SaveKeymapAs::ReadOnly)
        );
        assert_eq!(
            std::fs::read(&read_only).expect("read-only bytes"),
            b"read-only sentinel"
        );
        let mut permissions = std::fs::metadata(&read_only)
            .expect("metadata")
            .permissions();
        #[allow(clippy::permissions_set_readonly_false)] // restores a Windows read-only attribute
        permissions.set_readonly(false);
        std::fs::set_permissions(&read_only, permissions).expect("restore writable attribute");
        std::fs::remove_dir_all(&dir).expect("disposable keymap directory clears");
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn targeted_map_owns_one_priority_2000_callback_pair_and_leaves_other_owners() {
        // Entering target mode registers input map 0x1000000B at priority 2000 for this callback.
        let mut shell = InputShell::new(&store(), None).expect("required shipped maps");
        let other = shell.manager.new_callback();
        shell
            .manager
            .register_input_map(TARGET_INPUT_MAP, 1999, other);
        let before = shell.manager.maps.entries().to_vec();
        shell.set_target_input_map(true);
        shell.set_target_input_map(true);
        let own: Vec<_> = shell
            .manager
            .maps
            .entries()
            .iter()
            .filter(|e| e.callback == shell.target_callback)
            .collect();
        assert_eq!(own.len(), 1);
        assert_eq!((own[0].map, own[0].priority), (TARGET_INPUT_MAP, 2000));
        let mut pump = crate::pump::Pump::new();
        let press = pump
            .button_message(dereth_input::keys::MouseButton::Left, true, 1000)
            .unwrap();
        assert!(shell.on_message(press));
        let events = shell.manager.take_events();
        assert!(events
            .iter()
            .any(|e| e.input_map == TARGET_INPUT_MAP && e.action.0 == 7 && e.start));
        shell.set_target_input_map(false);
        shell.set_target_input_map(false);
        assert_eq!(shell.manager.maps.entries(), before);
        assert!(!shell.target_input_map_active());
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn ending_and_beginning_a_session_restores_the_constructors_walk_and_drops_held_keys() {
        let mut shell = InputShell::new(&store(), None).expect("the input tables load");
        shell.set_combat_input_maps(dereth_input::combat::mode::MELEE);
        let in_melee = shell.manager.maps.entries().to_vec();
        let mut pump = crate::pump::Pump::new();
        shell.on_message(pump.key_message_for_key(Key::KEY_W, true, 1_000));
        assert!(shell.is_action_in_progress(action::MOVE_FORWARD));

        assert!(shell.set_character_session_input_maps(false));
        assert!(
            !shell.set_character_session_input_maps(false),
            "an edge, not a level"
        );
        shell.finish_session_retirement(|_| {});
        let maps: Vec<u32> = shell
            .manager
            .maps
            .entries()
            .iter()
            .map(|e| e.map.0)
            .collect();
        assert_eq!(
            maps,
            [5, dereth_input::dereth::INPUT_MAP.0, 3, 0x10],
            "only the whole-run maps stay, in their walk order"
        );
        assert!(
            !shell.is_action_in_progress(action::MOVE_FORWARD),
            "a key held through the end of the session is not held on the character screen"
        );
        // No combat or target map can come up outside a session.
        assert!(!shell.set_combat_input_maps(dereth_input::combat::mode::MAGIC));
        shell.set_target_input_map(true);
        assert!(!shell.target_input_map_active());
        assert_eq!(shell.manager.maps.len(), WHOLE_RUN_INPUT_MAPS.len());

        assert!(shell.set_character_session_input_maps(true));
        shell.set_combat_input_maps(dereth_input::combat::mode::MELEE);
        assert_eq!(
            shell.manager.maps.entries(),
            in_melee,
            "the session's maps come back walked exactly as the constructor and the mode swap left them"
        );
    }

    /// Behaviour: keymap.lifecycle.interface-switch-and-session-end-release-runtime-movement
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn interface_switch_and_session_end_release_runtime_movement() {
        use dereth_client_runtime::character::{CharacterInput, MovementCommands};
        let mut shell = InputShell::new(&store(), None).expect("input tables");
        let mut pump = crate::pump::Pump::new();
        let mut movement = MovementCommands::default();
        let mut state = CharacterInput::default();
        let apply = |events: Vec<dereth_client_contract::actions::Action>,
                     movement: &mut MovementCommands,
                     state: &mut CharacterInput| {
            for event in events {
                movement.on_action(
                    dereth_client_runtime::actions::movement::on_action(
                        &event,
                        dereth_client_runtime::actions::emote::command_for_action,
                    ),
                    state,
                );
            }
        };
        for (index, next_classic) in [(0_u32, true), (1, false)] {
            shell.on_message(pump.key_message_for_key(Key::KEY_W, true, 1000 + index * 100));
            shell.collect_message();
            apply(
                shell
                    .take_events()
                    .iter()
                    .map(InputEvent::to_action)
                    .collect(),
                &mut movement,
                &mut state,
            );
            assert!(
                state.forward,
                "the physical press reached the runtime consumer"
            );
            apply(shell.release_actions(), &mut movement, &mut state);
            assert!(
                !state.forward,
                "the outgoing scope stops movement before replacement"
            );
            shell.activate_classic(next_classic);
            shell.on_message(pump.key_message_for_key(Key::KEY_W, false, 1050 + index * 100));
        }
        shell.on_message(pump.key_message_for_key(Key::KEY_W, true, 1500));
        shell.collect_message();
        apply(
            shell
                .take_events()
                .iter()
                .map(InputEvent::to_action)
                .collect(),
            &mut movement,
            &mut state,
        );
        assert!(state.forward);
        assert!(shell.set_character_session_input_maps(false));
        shell.finish_session_retirement(|events| {
            apply(events, &mut movement, &mut state);
            assert!(
                !state.forward,
                "runtime receives the release before callback retirement"
            );
        });
        assert!(!shell
            .manager
            .maps
            .entries()
            .iter()
            .any(|entry| entry.callback == shell.session_callback));
        assert!(
            shell.release_actions().is_empty(),
            "no second delivery after retirement"
        );
        assert!(shell.set_character_session_input_maps(true));
        shell.on_message(pump.key_message_for_key(Key::KEY_W, false, 1600));
        shell.on_message(pump.key_message_for_key(Key::KEY_W, true, 1700));
        shell.collect_message();
        shell.defer_declined_actions();
        assert!(shell.set_character_session_input_maps(false));
        assert!(
            shell.set_character_session_input_maps(true),
            "a new session can precede the retirement drain"
        );
        let mut delivered = Vec::new();
        shell.finish_session_retirement(|events| delivered = events);
        let phases: Vec<_> = delivered
            .iter()
            .filter(|event| event.id == action::MOVE_FORWARD)
            .map(|event| event.phase)
            .collect();
        assert_eq!(
            phases,
            [
                dereth_client_contract::actions::ActionPhase::Begin,
                dereth_client_contract::actions::ActionPhase::End
            ]
        );
        apply(delivered, &mut movement, &mut state);
        assert!(
            !state.forward,
            "a press and teardown in one batch retain their order"
        );
        assert!(
            shell
                .manager
                .maps
                .entries()
                .iter()
                .any(|entry| entry.callback == shell.session_callback),
            "the new session survives the old retirement"
        );
    }

    /// Behaviour: options.key-bindings.default-file-has-a-friendly-label
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn default_keymap_label_keeps_real_filenames_and_named_schemes() {
        let mut input = InputShell::new(&store(), None).unwrap();
        assert_eq!(input.keymap_display_name().as_deref(), Some("Default"));
        assert_eq!(input.keymap_file_name(), None);
        assert_eq!(input.keymap_path(), None);
        for (file, label) in [
            ("dereth-modern.keymap", "Default"),
            ("acclient.keymap", "Default"),
            ("Quest-modern.keymap", "Quest"),
            ("custom.keymap", "custom.keymap"),
        ] {
            input.keymap_path = Some(std::path::PathBuf::from(file));
            assert_eq!(input.keymap_display_name().as_deref(), Some(label));
            assert_eq!(input.keymap_file_name().as_deref(), Some(file));
            assert_eq!(input.keymap_path(), Some(std::path::Path::new(file)));
        }
    }

    /// The classic key page is where this client's own actions take a key: one already in use
    /// raises the question first, and once it is answered yes the key is taken from the other
    /// action and answers this client's.
    ///
    /// Behaviour: keys.own.a-key-in-use-given-to-this-clients-action-asks-first-and-is-taken
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_key_in_use_given_to_this_clients_action_asks_first_and_is_taken() {
        use dereth_classic_ui::keybindings::{CaptureResult, KeyBindings};
        use dereth_classic_ui::panels::HostAction;
        const VK_W: u16 = 0x57;
        let perf = dereth_client_contract::actions::dereth::TOGGLE_PERFORMANCE_PANEL;
        let own = dereth_input::dereth::INPUT_MAP;
        let mut shell = InputShell::new(&store(), None).expect("input tables");
        shell.activate_classic(true);
        let mut page = KeyBindings::new(&shell.classic_keys());
        assert!(
            page.bound_to(VK_W, 0, action::MOVE_FORWARD.0),
            "the key starts out walking forward"
        );
        page.handle(&HostAction::CaptureBinding {
            action: perf.0,
            map: own.0,
            slot: 0,
        })
        .expect("the row takes a key");
        let asked = page.key(VK_W, true, false, 0).expect("a key").capture;
        assert_eq!(asked, CaptureResult::Conflict("Walk Forward".into()));
        assert!(
            page.requests.is_empty(),
            "nothing is taken before the answer"
        );
        page.confirm_capture(true).expect("yes");
        for request in std::mem::take(&mut page.requests) {
            shell.classic_request(request);
        }
        let w = |keys: Vec<dereth_input::ControlChord>| {
            keys.iter()
                .any(|k| k.control.offset() == 0x11 && k.meta_mode == 0)
        };
        assert!(w(shell.keys_for_action(perf, own)));
        assert!(!w(shell.keys_for_action(
            action::MOVE_FORWARD,
            dereth_input::maps::MOVEMENT
        )));

        let mut pump = crate::pump::Pump::new();
        shell.on_message(pump.key_message_for_key(Key::KEY_W, true, 1_000));
        shell.collect_message();
        let answered: Vec<ActionId> = shell.take_events().iter().map(|e| e.action).collect();
        assert!(answered.contains(&perf), "{answered:?}");
        assert!(!shell.is_action_in_progress(action::MOVE_FORWARD));
    }

    /// Behaviour: keymap.storage.saved-maps-remain-specific-to-their-interface
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn stored_maps_remain_face_specific_while_classic_is_active() {
        use dereth_classic_ui::keystore::KeyStoreRequest;
        let dir = std::env::temp_dir().join(format!("dereth-face-maps-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("disposable directory");
        let path = dir.join("current-modern.keymap");
        let mut shell = InputShell::new(&store(), Some(&path)).expect("input tables");
        shell.activate_classic(true);
        shell.classic_request(KeyStoreRequest::Clear {
            scan: 0x11,
            modifiers: 0,
            action: action::MOVE_FORWARD.0,
            map: 4,
        });
        shell.classic_request(KeyStoreRequest::Bind {
            scan: 0x2f,
            modifiers: 0,
            action: action::MOVE_FORWARD.0,
            map: 4,
            replaced: None,
        });
        let classic = shell.manager.keymap.to_keymap_text();
        let mut chosen = shell.modern_map().clone();
        let key = dereth_input::scheme::keyboard_key(&chosen, 0x11, 0).expect("W control");
        dereth_input::scheme::clear(&mut chosen, InputMapId(4), key, action::MOVE_FORWARD);
        let file = dir.join("chosen-modern.keymap");
        std::fs::write(&file, chosen.to_keymap_text()).expect("chosen map");
        assert!(shell
            .load_keymap_file("chosen-modern.keymap")
            .expect("load Modern while Classic active"));
        assert_eq!(
            shell.manager.keymap.to_keymap_text(),
            classic,
            "Modern load cannot replace Classic's live map"
        );
        assert_eq!(
            (
                shell.modern_map().did,
                shell.modern_map().name.as_str(),
                shell.modern_map().guid
            ),
            (0, "User Defined Keymap", [0; 16]),
            "a named load creates a fresh document"
        );
        let modern = shell.modern_map().to_keymap_text();
        assert_ne!(modern, classic);
        shell.save_keymap().expect("shutdown save addresses Modern");
        assert_eq!(std::fs::read_to_string(&file).expect("saved map"), modern);
        assert_eq!(
            shell.save_keymap_as("copy", false).expect("save as"),
            Some(SaveKeymapAs::Saved)
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("copy-modern.keymap")).expect("named map"),
            modern
        );
        shell.activate_classic(false);
        assert_eq!(shell.manager.keymap.to_keymap_text(), modern);
        shell.activate_classic(true);
        assert_eq!(shell.manager.keymap.to_keymap_text(), classic);
        let mut reloaded = InputShell::new(&store(), Some(&file)).expect("reload both saved faces");
        reloaded.activate_classic(true);
        assert_eq!(
            reloaded.manager.keymap.to_keymap_text(),
            classic,
            "saved clear and replacement survive defaults merge"
        );
        reloaded.activate_classic(false);
        // Startup takes its document identity from the default map, while loading a named
        // scheme uses a user-document header. Compare the complete bindings and devices.
        reloaded.manager.keymap.name = shell.modern_map().name.clone();
        reloaded.manager.keymap.guid = shell.modern_map().guid;
        assert_eq!(reloaded.manager.keymap.to_keymap_text(), modern);
        for file in [
            "current-modern.keymap",
            CLASSIC_KEYMAP_FILE,
            "chosen-modern.keymap",
            "copy-modern.keymap",
        ] {
            let path = dir.join(file);
            if path.exists() {
                std::fs::remove_file(path).expect("remove disposable map");
            }
        }
        std::fs::remove_dir(dir).expect("remove disposable directory");
    }

    fn store() -> dereth_dat::RetailDatStore {
        let dir = dereth_dat::testing::dat_dir();
        assert!(
            dereth_dat::testing::have_dats(),
            "the retail dats are these tests' oracle and there are none at {} -- \
             set DERETH_TEST_DAT_DIR",
            dir.display()
        );
        dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail dats open")
    }

    // Oracle: the retail default key map (`0x14000000`, resolved through KEYMAP group 10 entry
    // `0x10000001`), dumped by an independent reader of the shipped dats: input map **4** binds
    // DIK 0x11 -- `DIK_W` -- with meta-mode 0 to action **41**, and `dereth_client_runtime::actions::movement::action`
    // names 41 (0x29) `MOVE_FORWARD`. Player input registers map 4 at the gameplay priority.
    //
    // This is the whole of "a synthetic window message reaches `dereth-input` and produces the mapped
    // action", and it is driven through the application's own path: `crate::pump::Pump` builds
    // the `MSG` from a host key transition and `InputShell::on_message` forwards it.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_synthetic_w_keydown_resolves_to_move_forward_through_the_retail_keymap() {
        let store = store();
        let mut shell = InputShell::new(&store, None).expect("the input tables load");

        // The `MSG` the pump builds for a host `KeyW` press, byte for byte.
        let mut pump = crate::pump::Pump::new();
        let down = pump.key_message_for_key(Key::KEY_W, true, 1_000);
        assert_eq!(down.message, msg::WM_KEYDOWN);
        assert_eq!(down.wparam, 0x57, "VK_W");
        assert_eq!(
            dereth_input::win32::keyboard_offset(down.lparam),
            Some(0x11),
            "the pump must carry DIK_W in the lParam, not only VK_W in the wParam"
        );

        assert!(
            shell.on_message(down),
            "the window procedure forwards WM_KEYDOWN and the message handler handles it"
        );
        assert_eq!(shell.stats.keyboard_without_scan_code, 0);

        let fired = shell.manager.take_events();
        assert_eq!(fired.len(), 1, "one press, one action: {fired:?}");
        assert_eq!(
            fired[0].action,
            action::MOVE_FORWARD,
            "DIK_W in map 4 is action 41"
        );
        assert_eq!(
            fired[0].input_map,
            InputMapId(4),
            "the player's movement-command map"
        );
        assert!(fired[0].start, "a press starts the action");
        assert!(shell.is_action_in_progress(action::MOVE_FORWARD));

        // ...and the release ends it, with the same control.
        let up = pump.key_message_for_key(Key::KEY_W, false, 1_100);
        assert!(shell.on_message(up));
        let fired = shell.manager.take_events();
        assert_eq!(fired.len(), 1, "{fired:?}");
        assert_eq!(fired[0].action, action::MOVE_FORWARD);
        assert!(!fired[0].start, "a release ends it");
        assert!(!shell.is_action_in_progress(action::MOVE_FORWARD));
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_typing_barrier_stops_w_walking_only_while_a_text_box_has_focus() {
        let dir = dereth_dat::testing::dat_dir();
        assert!(
            dereth_dat::testing::have_dats(),
            "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
            dir.display()
        );
        let store = dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail dats open");
        let mut shell = InputShell::new(&store, None).expect("the input tables load");
        let mut pump = crate::pump::Pump::new();
        let t = std::cell::Cell::new(1_000u32);

        // One press-and-release of `W`, answering "did it produce MOVE_FORWARD?".
        let press_w = |shell: &mut InputShell, pump: &mut crate::pump::Pump| {
            let mut walked = false;
            for down in [true, false] {
                t.set(t.get() + 100);
                let m = pump.key_message_for_key(Key::KEY_W, down, t.get());
                shell.on_message(m);
                walked |= shell
                    .manager
                    .take_events()
                    .iter()
                    .any(|e| e.action == action::MOVE_FORWARD);
            }
            walked
        };

        // (a) Nothing focused: `w` walks. Without this half the test passes on a client that
        // blocks movement for ever.
        assert!(
            press_w(&mut shell, &mut pump),
            "with no text box focused, W is the movement key"
        );

        // (b) A text box takes focus and pushes **four** maps. It first registers scrollable map
        // 0x0A unconditionally, then registers maps 1, 7, and 8 according to its bit-field flags.
        // The input-map list prepends an equal-priority newcomer, so the walk order is the reverse
        // of the registration order.
        shell.set_focused_input_maps(&FOCUSED_TEXT_MAP_REGISTRATIONS);
        let band: Vec<u32> = shell
            .manager
            .maps
            .entries()
            .iter()
            .filter(|e| {
                e.priority == dereth_input::dispatch::priority::FOCUSED_UI
                    || e.priority == dereth_input::dispatch::priority::FOCUSED_UI_TEXT_BARRIER
            })
            .map(|e| e.map.0)
            .collect();
        assert_eq!(
            band,
            vec![8, 7, 10, 1],
            "registering 0x0A, 1, 7, 8 walks as 8, 7, 0x0A, 1. Maps 8, 7 and \
             0x0A go in at 3000 and prepend; map 1 goes in at 2990 \
             and sorts below all three, so the keyboard barrier really is \
             LAST and nothing a focused text element registered sits behind it"
        );
        assert!(
            !press_w(&mut shell, &mut pump),
            "with the entry focused, W must not walk the character"
        );

        // ...and it is a *keyboard* barrier: `set_text_mode` is on, so the same key still arrives as
        // a character for the focused text element to insert.
        shell.set_text_mode(true);
        t.set(t.get() + 100);
        shell.on_message(dereth_input::win32::Win32Message::new(
            msg::WM_CHAR,
            'w' as usize,
            0,
            t.get(),
        ));
        shell.use_time(dereth_primitives::LocalTime(f64::from(t.get()) / 1000.0));
        assert_eq!(
            shell.take_characters(),
            vec!['w'],
            "the box still gets its 'w'"
        );

        // (c) The box loses focus: `unregister_callback` takes all three maps with it and
        // movement comes back.
        shell.set_focused_input_maps(&[]);
        shell.set_text_mode(false);
        assert!(
            shell
                .manager
                .maps
                .entries()
                .iter()
                .all(|e| e.map != InputMapId(1)),
            "the barrier is gone with the focus"
        );
        assert!(
            press_w(&mut shell, &mut pump),
            "unfocusing the box restores movement"
        );
    }

    fn ctrl_up(shell: &mut InputShell, pump: &mut crate::pump::Pump, t: &mut u32) -> Vec<u32> {
        for code in [Key::CONTROL_LEFT, Key::ARROW_UP] {
            *t += 100;
            let m = pump.key_message_for_key(code, true, *t);
            shell.on_message(m);
        }
        let fired: Vec<u32> = shell
            .manager
            .take_events()
            .iter()
            .map(|e| e.action.0)
            .collect();
        for code in [Key::ARROW_UP, Key::CONTROL_LEFT] {
            *t += 100;
            let m = pump.key_message_for_key(code, false, *t);
            shell.on_message(m);
        }
        let _ = shell.manager.take_events();
        fired
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn map_0x0a_answers_the_wheel_and_not_ctrl_arrow_while_a_text_box_has_focus() {
        let store = store();
        let mut shell = InputShell::new(&store, None).expect("the input tables load");
        let mut pump = crate::pump::Pump::new();
        let mut t = 2_000u32;

        // `ScrollUp` / `ScrollDown`, as literals.
        const SCROLL_UP: u32 = 5;
        const SCROLL_DOWN: u32 = 6;

        shell.set_focused_input_maps(&FOCUSED_TEXT_MAP_REGISTRATIONS);
        let _ = shell.manager.take_events();

        // The control: with **nothing** focused, map 0x0A is not registered at all and the
        // same gesture must reach nothing. Without it the assertion below would also pass on
        // a build with no barrier whatsoever.
        shell.set_focused_input_maps(&[]);
        let unfocused = ctrl_up(&mut shell, &mut pump, &mut t);
        eprintln!("Ctrl+UP with nothing focused fired {unfocused:?}");
        assert!(
            !unfocused.contains(&SCROLL_UP) && !unfocused.contains(&SCROLL_DOWN),
            "with no focused element there is no ScrollableControls to answer Ctrl+DIK_UP"
        );

        shell.set_focused_input_maps(&FOCUSED_TEXT_MAP_REGISTRATIONS);
        let _ = shell.manager.take_events();

        // Ctrl held, then the up arrow -- the exact qualified control ScrollableControls
        // binds.
        let by_key = ctrl_up(&mut shell, &mut pump, &mut t);
        eprintln!("Ctrl+UP with a text box focused fired {by_key:?}");
        assert!(
            by_key.contains(&SCROLL_UP),
            "Ctrl+DIK_UP is bound in map 0x0A, which walks AHEAD of MAP_BLOCK_KEYBOARD \
             because the barrier goes in at 2990 and 0x0A at 3000, so it \
             reaches ScrollUp even while a text box has focus"
        );

        // The same map and the same two actions, through the mouse. The `wParam` packing is
        // `Pump::map_window_event`'s own: WHEEL_DELTA 120 in the high word.
        t += 100;
        shell.on_message(dereth_input::win32::Win32Message::new(
            msg::WM_MOUSEWHEEL,
            (120u32 << 16) as usize,
            0,
            t,
        ));
        let by_wheel: Vec<u32> = shell
            .manager
            .take_events()
            .iter()
            .map(|e| e.action.0)
            .collect();
        eprintln!("one WM_MOUSEWHEEL detent fired {by_wheel:?}");
        assert!(
            by_wheel.contains(&SCROLL_UP) || by_wheel.contains(&SCROLL_DOWN),
            "the wheel is a mouse control, so walk_input_maps lets it past the keyboard barrier \
             and map 0x0A must answer it"
        );

        // And the registration is the focus's: with the focus gone the wheel resolves to nothing,
        // which is what `-Wheel 2` measured live as 0 px after the box was unfocused.
        shell.set_focused_input_maps(&[]);
        t += 100;
        shell.on_message(dereth_input::win32::Win32Message::new(
            msg::WM_MOUSEWHEEL,
            (120u32 << 16) as usize,
            0,
            t,
        ));
        let unfocused: Vec<u32> = shell
            .manager
            .take_events()
            .iter()
            .map(|e| e.action.0)
            .collect();
        assert!(
            !unfocused.contains(&SCROLL_UP) && !unfocused.contains(&SCROLL_DOWN),
            "0x0A is registered from the focus and unregistered with it -- it fired {unfocused:?}"
        );
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_pumps_message_timing_is_what_decides_a_double_click() {
        let store = store();
        let dbl = |gap: u32| {
            let mut shell = InputShell::new(&store, None).expect("the input tables load");
            let t0 = 500_000u32;
            for t in [t0, t0 + gap] {
                shell.on_message(dereth_input::win32::Win32Message::new(
                    msg::WM_LBUTTONDOWN,
                    0,
                    0,
                    t,
                ));
                shell.on_message(dereth_input::win32::Win32Message::new(
                    msg::WM_LBUTTONUP,
                    0,
                    0,
                    t + 1,
                ));
            }
            shell.manager.history.get(dereth_input::ControlCode::new(
                shell
                    .manager
                    .keymap
                    .devices
                    .iter()
                    .position(|d| d.device_type == dereth_input::DeviceType::Mouse)
                    .and_then(|i| u8::try_from(i).ok())
                    .unwrap_or(1),
                dereth_input::SubControlIndex::None,
                0x0C,
            ))
        };
        let threshold = InputManager::empty().timing.double_click_ms;
        assert!(
            threshold > 0,
            "a zero threshold would make the assertion vacuous"
        );
        // Inside the threshold the second press pairs with the first and the history entry is
        // consumed by the double-click release; outside it, the entry survives as a fresh press.
        assert!(
            dbl(threshold + 5_000).is_some(),
            "a slow pair is two separate clicks"
        );
        assert!(
            dbl(threshold / 2).is_none(),
            "a fast pair is a double click"
        );
    }

    // Oracle: the window message table swallows `SC_SCREENSAVE` and
    // `SC_MONITORPOWER` are swallowed and never reach the input manager, and everything the table
    // does not name falls through to `DefWindowProcA`.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn only_the_messages_the_table_forwards_reach_the_input_manager() {
        let store = store();
        let mut shell = InputShell::new(&store, None).expect("the input tables load");
        let m = |message, wparam| dereth_input::win32::Win32Message::new(message, wparam, 0, 1);
        assert!(!shell.on_message(m(msg::WM_SYSCOMMAND, msg::SC_SCREENSAVE)));
        assert!(!shell.on_message(m(msg::WM_SYSCOMMAND, msg::SC_MONITORPOWER)));
        assert!(
            !shell.on_message(m(0x0007_0000, 0)),
            "an unnamed message is DefWindowProc's"
        );
        assert_eq!(shell.stats.messages_handled, 0);
        assert_eq!(shell.stats.messages_offered, 3);

        // ...and the ones it does forward are handled. `WM_SETFOCUS` and `WM_MOUSEMOVE` are both
        // `WndProcDisposition::Forward`, and the message handler sets its handled flag up front and
        // clears it only in the default case.
        assert!(shell.on_message(m(msg::WM_SETFOCUS, 0)));
        let move_msg = dereth_input::win32::Win32Message::new(msg::WM_MOUSEMOVE, 0, 0x0064_0064, 2);
        assert!(shell.on_message(move_msg));
        assert_eq!(
            shell.manager.mouse_pos(),
            (100, 100),
            "the mouse position is stored, not dispatched"
        );
        assert_eq!(shell.stats.messages_handled, 2);
    }
}
