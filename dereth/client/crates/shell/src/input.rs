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

/// The keyboard UI's three non-error keymap-save outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveKeymapAs {
    Saved,
    NeedsOverwrite,
    ReadOnly,
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
    /// input manager cannot reach the UI, so the message is recorded here and `dereth_client::ui::UiShell` makes the call.
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

/// The input maps a bare client must register for a key to reach an action at all.
///
/// A subset of `dereth_input::RETAIL_MAP_REGISTRATIONS` — the rows whose owner exists in this build.
/// A row is added when its owner exists. The subset is named here rather than filtered at run
/// time so that what is missing is visible.
///
/// | owner | map | priority | what it carries |
/// |---|---:|---:|---|
/// | client UI initialization | `0x10` | −1 | the system-key swallow, always last |
/// | UI element manager | 3 | 0 | the UI's own actions |
/// | `CameraManager` | 5 | 1000 | camera |
/// | player input | 4 | 1000 | **movement** — this is the map `W` is in |
/// | combat input | `0x10000002` | 1000 | `Combat` — `CombatToggleCombat` and the power-bar keys |
/// | player input | `0x10000007` | 1000 | `ItemSelectionCommands` — `USE`, `SelectionExamine`, the target cycle |
/// | UI input | `0x10000009` | 1000 | `UICommands` — the panel toggles, `CaptureScreenshot`, and **`Shift+DIK_ESCAPE` → `0x10000026` End Character Session** |
/// | UI input | `0x1000000C` | 1000 | `QuickslotCommands` — the eighteen quickbar hotkeys and `CreateShortcut` |
/// | `MainChat` | `0x1000000D` | 3010 | `DIK_TAB` → `ToggleChatEntry 0x10000024` — above `priority::FOCUSED_UI` so Tab still leaves the bar |
/// | `MainChat` | `0x1000000A` | 1000 | `DIK_RETURN` → `EnterChatMode 0x10000023` — the only key that opens a text field |
///
/// Without a registration a map is not in the priority-ordered walk at all, so without the
/// combat and item-selection rows every combat and selection binding in the shipped `DefaultMap`
/// would resolve to nothing, and there would be no way to enter combat mode from the keyboard.
///
/// **The three combat mode maps are deliberately absent.** `0x10000003` (`MeleeCombat`),
/// `0x10000004` (`MissileCombat`) and `0x10000005` (`MagicCombat`) are all listed in
/// [`dereth_input::RETAIL_MAP_REGISTRATIONS`] — that table is a catalogue of *registration
/// sites*, and all three sites are the one function which registers **exactly one of them per
/// combat mode** and none at all in peace. Registered together they would be walked
/// `0x10000005, 0x10000004, 0x10000003` (the last registered is walked first), so `MagicCombat`
/// would win all five of the controls the three share — `DIK_INSERT`, `DIK_PRIOR`, `DIK_DELETE`,
/// `DIK_END`, `DIK_NEXT` — and the `CombatLow`/`Medium`/`HighAttack` arms in
/// `dereth_client::interaction::Interaction::on_action` could never receive a key. The swap is
/// [`InputShell::register_combat_input_maps`]; the shipped action-map conflict table is
/// the independent second reading and it omits exactly those three pairs.
///
/// **`0x1000000C` carries 19 actions over 29 shipped controls**, the largest single block of
/// default keys that would be dead without it. `1`–`9` and `Ctrl+1`–`9` are
/// `UseQuickSlot_1`…`_9`, `Alt+1`–`9` are `UseQuickSlot_10`…`_18`, and `0` / `Ctrl+0` are
/// the create-shortcut action. Beginning a character session registers `0x10000009`
/// and then `0x1000000C`, both at priority 1000, and the order matters: registering the quickslot
/// map second puts it **ahead** of `UICommands`, which is what makes `Alt+1`…`Alt+4` reach
/// `UseQuickSlot_10`…`_13` rather than `ToggleFloatingChatWindow1`…`4`. That pair is the one place
/// in the shipped data where the conflicting-maps table says two co-registered maps **do**
/// conflict, so the clash is retail's own and the registration order is the whole of its
/// resolution.
///
/// **`0x10000009` is how a player leaves the game.** The shipped game keymap `0x14000000` binds
/// `Shift+DIK_ESCAPE` to `0x10000026` *End Character Session* in that map and **nowhere else** —
/// 20 bindings, and that is the last of them. Gameplay key handling and the epilogue behind it
/// are unreachable from a keyboard unless the map carrying the binding is registered.
///
/// **`CameraManager` map 6 is deliberately absent, because it would move the arrow keys.** Map 6
/// is the *alternate camera* map, registered by camera action handling for **action 0x3E only**.
/// It is registered on the press of
/// Toggle Alternate Camera Mode and unregistered on the release, for keyboards with no numeric
/// keypad. Registered at startup instead, it would sit at `priority::UNFOCUSED_UI` (2000) **above**
/// map 4 at `priority::GAMEPLAY` (1000), and `walk_input_maps` keeps the first of two equally-good
/// matches — so `DIK_UP`/`DOWN`/`LEFT`/`RIGHT`, which map 4 binds to Move Forward / Move Backward /
/// Turn Left / Turn Right and map 6 binds to Rotate Camera Up/Down/Left/Right, would resolve to
/// the **camera** and spin the view instead of walking. It lives on `0x3E`'s own arm in
/// `App::apply_world_camera_action`, which is where the client puts it.
///
/// UI map `0x1000000B` (`TargetedUsage`) has its live mode-edge owner in
/// [`InputShell::set_target_input_map`]. Player input map `0x10000006` (`Emotes`) is owned by the
/// command interpreter, through `crate::actions::emote::command_for_action`. Player input map
/// `0x10000008` (`CharacterOptionCommands`) binds nothing in the shipped keymaps, but a key the
/// player gives a character option goes there, and the player system's registration is what lets
/// it reach the option. Still absent: the debug maps, which have no owner in this build.
///
/// **Input map 9 (`DialogBoxes`) is not here and does not belong here.** It has no
/// start-up owner at all: the original intro, credits and character-management constructors are
/// its three registrants, so it is live only while one of those screens is
/// up and goes away with the screen. It is mirrored on the mode edge instead, by
/// [`InputShell::set_mode_input_maps`] from `dereth_client::ui::PREGAME_MODE_INPUT_MAPS`. Putting it in
/// this table would register it for the whole session and give `DIK_RETURN` to
/// `AcceptInput` in the game world, where retail gives it to `EnterChatMode`.
/// The four input maps the text element registers **while it has focus**, in order:
/// the **scrollable** map (`0x0A`,
/// `ScrollableControls`), then the keyboard **barrier** (map 1,
/// [`dereth_input::MAP_BLOCK_KEYBOARD`]) and then the two maps that carry the text actions
/// `0x16`…`0x28`. See `InputShell::set_focused_text_maps`.
///
/// **`0x0A` is first and unconditional, and it carries the mouse wheel:**
///
/// ```text
/// Text element: register the scrollable base maps first, including `0x0A`.
///   if editable (flag bit 1):             register maps 1 and 7
///   if editable or selectable (bits 1|4): register map 8
///
/// Scrollable element: register the base element's maps, including its own input map if set,
///   then register map 0x0A unconditionally.
/// ```
///
/// The base registration has no flag test at all, so a focused text element registers
/// `0x0A` whether it is editable, selectable or neither. `ScrollableControls` is the **only**
/// section of the shipped merged keymap that binds `DIMOFS_WHEEL`
/// (the merged keymap's `ScrollableControls` section: `ScrollUp` / `ScrollDown` on the
/// wheel's two axis halves, plus `Ctrl+DIK_UP` / `Ctrl+DIK_DOWN`), so **without this entry a real
/// wheel detent resolves to no action whatsoever**. The scrollable element's second message
/// arm and the scrollbar's mouse-wheel handler reflect `0x0D`/`0x0E`, but those downstream
/// handlers are unreachable in the running client without it. The UI crate's wheel tests cannot
/// see this constant; the client's routing tests can.
///
/// The order here is the client's **registration** order and it is not the walk order. Two
/// mechanisms decide the walk:
///
/// * [`dereth_input::dispatch::InputMapStack::register`] puts an equal-priority newcomer **in
///   front**, matching the client's sorted-list insertion;
/// * [`focused_map_priority`] puts map 1 at **2990** and the other three at 3000, because
///   text-map registration subtracts 10 from the supplied priority for the typing barrier.
///
/// So registering `0x0A, 1, 7, 8` walks as **`8, 7, 0x0A, 1`** — the barrier genuinely last.
///
/// **The barrier's 2990 matters.** With the barrier third (`8, 7, 1, 0x0A`),
/// `ScrollableControls`' `Ctrl+DIK_UP` / `Ctrl+DIK_DOWN` — which are **keyboard** controls —
/// would stay blocked while a text box had focus. At 2990 they reach `ScrollableControls` and
/// scroll the focused element, which is what retail does. The wheel is unaffected in either
/// arrangement, because `walk_input_maps` lets **mouse** controls past map 1 wherever it sits.
pub const FOCUSED_TEXT_MAPS: [u32; 4] = [0x0A, 1, 7, 8];

/// `ScrollableControls`, the map registered by the scrollable element and
/// the only section of the shipped merged keymap that binds `DIMOFS_WHEEL`.
///
/// Named because two files need the same number for two different reasons: it is
/// [`FOCUSED_TEXT_MAPS`]'s first entry here, and it is the one map besides
/// `dereth_client::ui::UI_INPUT_MAP` whose events the UI shell answers itself — see
/// `UiShell::reaches_the_managers_on_action` for why that is a declared deviation rather than a
/// transcription.
pub const SCROLLABLE_INPUT_MAP: dereth_input::InputMapId = dereth_input::InputMapId(0x0A);
pub const TARGET_INPUT_MAP: dereth_input::InputMapId = dereth_input::InputMapId(0x1000_000B);

/// **The priority each of [`FOCUSED_TEXT_MAPS`] is registered at, which is not uniform.**
///
/// Focusing an element hands its map registration **3000**, and every map in the focused band
/// goes in at that value *except* the typing barrier:
/// Text-map registration computes map 1's priority as **the supplied priority − 10 = 2990**, while
/// maps 7, 8 and `0x0A` retain the supplied priority.
/// See [`dereth_input::dispatch::priority::FOCUSED_UI_TEXT_BARRIER`].
///
/// Keying on the map id rather than carrying a priority through
/// `dereth_client::ui::UiShell::focused_input_maps` is exact **here** and only here: map 1 has exactly
/// one registrant in this band — the text-map registration — while `DebugConsole` registers the same map at
/// its own [`dereth_input::dispatch::priority::DEBUG_CONSOLE_BARRIER`] through a different callback
/// and never through this function.
#[must_use]
pub const fn focused_map_priority(map: u32) -> i32 {
    use dereth_input::dispatch::priority;
    if map == dereth_input::MAP_BLOCK_KEYBOARD.0 {
        priority::FOCUSED_UI_TEXT_BARRIER
    } else {
        priority::FOCUSED_UI
    }
}

/// [`FOCUSED_TEXT_MAPS`] with the priority each is registered at, which is what
/// [`InputShell::set_focused_input_maps`] takes.
///
/// The pairs are not cosmetic. Text-map registration first calls
/// the base element's registration, whose priorities are the supplied priority for the
/// element's own input map and one lower for each generation above it
/// — a band that no `map -> priority` function can
/// answer, because the same map at two depths goes in at two priorities. So the focused set is
/// `(map, priority)` pairs, exactly as the active one is.
///
/// The four entries here still come from [`focused_map_priority`] rather than from four literals,
/// so the 2990 lives in one place; `the_focused_text_maps_are_the_four_the_client_pushes_in_the_clients_own_order`
/// pins both against retail.
pub const FOCUSED_TEXT_MAP_REGISTRATIONS: [(u32, i32); 4] = [
    (
        FOCUSED_TEXT_MAPS[0],
        focused_map_priority(FOCUSED_TEXT_MAPS[0]),
    ),
    (
        FOCUSED_TEXT_MAPS[1],
        focused_map_priority(FOCUSED_TEXT_MAPS[1]),
    ),
    (
        FOCUSED_TEXT_MAPS[2],
        focused_map_priority(FOCUSED_TEXT_MAPS[2]),
    ),
    (
        FOCUSED_TEXT_MAPS[3],
        focused_map_priority(FOCUSED_TEXT_MAPS[3]),
    ),
];

/// **The rows of [`BASE_MAP_REGISTRATIONS`] that are live for the whole run.** The system-key
/// swallow, the UI's own mouse map and the camera are registered when their owners are built at
/// start-up, and stay until shutdown.
///
/// **Every other row is a character-session map.** The player, UI, combat and chat systems register
/// their maps when a character session begins — the player description arriving, which is also
/// when the gameplay screen comes up — and drop them when it ends. So on the intro, the character
/// screens, character creation and the disconnected screen, no key reaches movement, examine, use,
/// the panel toggles, the quickbar, emotes, combat or chat: `E` and `R` resolve to nothing there,
/// and the only keys a pre-game screen answers are its own map 9's Enter and Escape, plus whatever a
/// focused text box registers. See [`InputShell::set_character_session_input_maps`].
pub const WHOLE_RUN_INPUT_MAPS: [u32; 4] = [0x10, 3, 5, dereth_input::dereth::INPUT_MAP.0];

pub const BASE_MAP_REGISTRATIONS: &[(&str, u32, i32)] = &[
    (
        "Client",
        0x10,
        dereth_input::dispatch::priority::CLIENT_SYSTEM_KEYS,
    ),
    ("ui-manager", 3, dereth_input::dispatch::priority::LOWEST),
    // This client's own actions (the performance panel's key), live for the whole run below
    // every game map, so a player's own binding of the same key wins.
    (
        "Client",
        dereth_input::dereth::INPUT_MAP.0,
        dereth_input::dispatch::priority::LOWEST,
    ),
    ("camera", 5, dereth_input::dispatch::priority::GAMEPLAY),
    ("player", 4, dereth_input::dispatch::priority::GAMEPLAY),
    // Combat-system character-session startup registers map `0x10000002` with the
    // controller callback at priority `0x3e8`; it stays registered until the session ends.
    // **`0x10000003`/`4`/`5` are deliberately not here** — see the combat mode maps paragraph above
    // and [`InputShell::register_combat_input_maps`], which owns those transitions.
    (
        "combat",
        0x1000_0002,
        dereth_input::dispatch::priority::GAMEPLAY,
    ),
    (
        "player",
        0x1000_0007,
        dereth_input::dispatch::priority::GAMEPLAY,
    ),
    // The character options' map, which the player system registers beside the selection map.
    (
        "player",
        0x1000_0008,
        dereth_input::dispatch::priority::GAMEPLAY,
    ),
    // **The `Emotes` map, the player system's last input-map registration.**
    //
    // Both registrations in that function pass the same explicit priority:
    //
    // Register map 4 (MovementCommands) at priority 1000, then map 0x10000006
    // (Emotes) at that same priority, both using the smart box's command interpreter.
    //
    // Both are registered under the smart box's command interpreter and under the same guard, so
    // `Emotes` is live in exactly the states `MovementCommands` is. It is the client's own answer
    // to *"who listens for `Wave`?"* through
    // [`crate::actions::emote::INPUT_ACTION_COMMANDS`].
    //
    // Without this row all five of the shipped `Emotes` bindings -- `O` Cheer, `U` Cry, `I` Laugh,
    // `K` PointState, `J` Wave -- reach no map at all. No other registered map binds an unmodified
    // `DIK_O`/`U`/`I`/`K`/`J`, so registering the map shadows nothing.
    //
    // It goes here rather than beside the player movement-map row only because this list's existing
    // order already differs from retail's (which registers `0x10000007`, `0x10000008`, `4`,
    // `0x10000006`, and `register` prepends within a band, so retail's gameplay walk is
    // `6, 4, 8, 7` and ours is not). Nothing turns on it while no two of those maps bind the same
    // control, which the input binding gap tests assert rather than assume.
    (
        "player",
        0x1000_0006,
        dereth_input::dispatch::priority::GAMEPLAY,
    ),
    // UI character-session startup registers `0x10000009` and then `0x1000000C`,
    // both at priority 1000.
    (
        "client-ui",
        0x1000_0009,
        dereth_input::dispatch::priority::GAMEPLAY,
    ),
    (
        "client-ui",
        0x1000_000C,
        dereth_input::dispatch::priority::GAMEPLAY,
    ),
    // Main-chat initialization registers these two maps, carrying the only keys in the
    // shipped keymaps that open a text field: `DIK_TAB -> ToggleChatEntry 0x10000024` lives in
    // `0x1000000D` and `DIK_RETURN -> EnterChatMode 0x10000023` in `0x1000000A`, and **nothing
    // else in either keymap binds either action**. Without them chat-entry activation is
    // reachable only from a mouse click, so no key press can take a text field's focus and the
    // ignore-next-character latch has nothing to arm on.
    //
    // `0x1000000D` sits at `priority::TOGGLE_CHAT_ENTRY` (3010) **above** `priority::FOCUSED_UI`,
    // which is deliberate in the client and is what lets Tab still leave the chat bar once the
    // bar's own barrier (map 1) is up. `0x1000000A` is at `priority::GAMEPLAY`, so a focused text
    // box's map 7 — which binds the same `DIK_RETURN` to text action `0x25` — outranks it and
    // Enter inside the box still means "send", not "open".
    //
    // **Walk order.** `register` prepends within a band, so appending here puts `0x1000000A`
    // **first** in the `priority::GAMEPLAY` band: the walk becomes `0x1000000A, 0x10000009,
    // 0x10000007, 0x10000005, 0x10000004, 0x10000003, 0x10000002, 4, 5`. That is retail's own
    // relative order — `RETAIL_MAP_REGISTRATIONS` registers `MainChat`'s `0x1000000A` last of
    // the gameplay band too — and it shadows nothing, because `0x1000000A` binds exactly one
    // control and no other registered map binds an unmodified `DIK_RETURN`.
    (
        "main-chat",
        0x1000_000D,
        dereth_input::dispatch::priority::TOGGLE_CHAT_ENTRY,
    ),
    (
        "main-chat",
        0x1000_000A,
        dereth_input::dispatch::priority::GAMEPLAY,
    ),
];

/// Keymap initialization step 2: resolve where the `.keymap` file lives.
///
/// ```text
/// dir  = the user-preferences settings directory
/// name = Input.KeymapFile, or the running executable's file name with the extension -> "keymap"
/// path = (dir joined with name, true)
/// ```
///
/// `keymap_file` is the `Input.KeymapFile` preference, whose
/// help text is *"The filename of the keymap file to use"*. `None` or empty takes the default,
/// which is [`DEFAULT_KEYMAP_FILE`].
///
/// **The default is a constant, and retail's rule was not.** Retail's keymap initialization takes
/// the *running executable's* file name and swaps the extension for `keymap`, which is the only
/// reason retail's file is called `acclient.keymap`. A rule that reads the binary's name means renaming or copying the binary silently orphans the
/// player's bindings -- a `dereth-client-debug.exe` or a second copy under any other name comes up
/// with no keymap and writes a new one beside the old. The file is the player's, not the
/// executable's, so it gets a fixed name.
///
/// Returns `None` when `preferences_file` is empty, which is an empty keymap-file path and the one
/// state in which keymap cleanup writes nothing.
#[must_use]
pub fn keymap_path_for(
    preferences_file: &std::path::Path,
    keymap_file: Option<&str>,
) -> Option<PathBuf> {
    if preferences_file.as_os_str().is_empty() {
        return None;
    }
    let dir = preferences_file
        .parent()
        .map_or_else(PathBuf::new, std::path::Path::to_path_buf);
    let name = match keymap_file.filter(|s| !s.is_empty()) {
        Some(n) => PathBuf::from(n),
        None => PathBuf::from(DEFAULT_KEYMAP_FILE),
    };
    Some(dir.join(name))
}

/// The `.keymap` file a run with no `Input.KeymapFile` preference reads and writes, in the
/// settings directory. Retail's equivalent is `acclient.keymap`; see [`keymap_path_for`] for why
/// this is a constant and that one is not.
pub const DEFAULT_KEYMAP_FILE: &str = "dereth-modern.keymap";

/// The modern (retail) interface's slug: its key maps are `<name>-modern.keymap`.
pub const MODERN_SLUG: &str = "modern";
/// The classic interface's slug: its key maps are `<name>-classic.keymap`.
pub const CLASSIC_SLUG: &str = "classic";

/// The file of the key map `name` of the interface whose slug is `slug`: `<name>-<slug>.keymap`.
/// A name typed with the slug or the extension already on is taken without them.
#[must_use]
pub fn scheme_file(name: &str, slug: &str) -> String {
    let name = name.trim();
    let name = strip_suffix_ignore_case(name, ".keymap").unwrap_or(name);
    let tail = format!("-{slug}");
    let name = strip_suffix_ignore_case(name, &tail).unwrap_or(name);
    format!("{name}-{slug}.keymap")
}

/// The name of the key map in `file` when it is one of the interface whose slug is `slug`.
#[must_use]
pub fn scheme_name(file: &str, slug: &str) -> Option<String> {
    let tail = format!("-{slug}.keymap");
    strip_suffix_ignore_case(file, &tail)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
}

fn strip_suffix_ignore_case<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let at = s.len().checked_sub(suffix.len())?;
    (s.is_char_boundary(at) && s[at..].eq_ignore_ascii_case(suffix)).then(|| &s[..at])
}

/// The classic interface's key map file, beside the retail one.
pub const CLASSIC_KEYMAP_FILE: &str = "dereth-classic.keymap";

/// The classic interface's key map: its file read over its default scheme, defaults included,
/// as the retail key map is kept.
#[derive(Debug, Clone)]
pub struct ClassicKeymap {
    /// The map as it is now.
    pub map: dereth_input::MasterInputMap,
    /// The classic default scheme it is read over.
    pub defaults: dereth_input::MasterInputMap,
    /// Its file, `None` when the client keeps no files.
    pub path: Option<PathBuf>,
}

impl ClassicKeymap {
    /// Write the map to its file.
    ///
    /// # Errors
    /// [`dereth_input::InputError::Io`] if the file cannot be written.
    pub fn save(&self) -> Result<(), dereth_input::InputError> {
        if let Some(path) = &self.path {
            dereth_client_runtime::platform::files::write(path, self.map.to_keymap_text())?;
        }
        Ok(())
    }
}

/// The input manager's full merged keymap, written where the host keeps the client's files.
///
/// # Errors
/// [`dereth_input::InputError::Io`] if the file cannot be written.
fn write_keymap(
    map: &dereth_input::MasterInputMap,
    path: &std::path::Path,
) -> Result<(), dereth_input::InputError> {
    dereth_client_runtime::platform::files::write(path, map.to_keymap_text())?;
    Ok(())
}

/// Persist `Input.KeymapFile` with `WritePrivateProfileString`-style merge semantics.
pub fn save_keymap_preference(
    preferences_file: &std::path::Path,
    keymap_file: &str,
) -> Result<(), std::io::Error> {
    if preferences_file.as_os_str().is_empty() {
        return Ok(());
    }
    let mut ini = match dereth_client_runtime::platform::files::read_to_string(preferences_file) {
        Ok(text) => dereth_client_contract::persist::preferences::UserPreferences::parse(&text)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            dereth_client_contract::persist::preferences::UserPreferences {
                crlf: true,
                ..Default::default()
            }
        }
        Err(error) => return Err(error),
    };
    ini.write_profile_string("Input", "KeymapFile", keymap_file);
    dereth_client_runtime::platform::files::write(preferences_file, ini.to_text())
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
        if self.classic_active {
            &self.modern
        } else {
            &self.manager.keymap
        }
    }

    fn modern_map_mut(&mut self) -> &mut dereth_input::MasterInputMap {
        if self.classic_active {
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
                Some(ActionId(
                    dereth_client_contract::actions::dereth::REPEAT_LAST_MESSAGE,
                )),
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
            focused_maps: Vec::new(),
            mode_maps: Vec::new(),
            retiring_session: false,
            retiring_actions: Vec::new(),
            characters: Vec::new(),
        })
    }

    /// Entering a target mode registers map 0x1000000B under the UI's target-mode callback at
    /// priority 2000. Leaving it unregisters only this pair; do not clear another owner's held actions.
    ///
    /// Target mode is character-session state: outside a session there is none to enter, so a
    /// request to enter one then registers nothing, and the map cannot sit above the UI's own mouse
    /// map on a pre-game screen.
    pub fn set_target_input_map(&mut self, active: bool) {
        if self.target_input_map_active == active || (active && !self.character_session_input_maps)
        {
            return;
        }
        self.target_input_map_active = active;
        if active {
            self.manager
                .register_input_map(TARGET_INPUT_MAP, 2000, self.target_callback);
        } else {
            self.manager
                .unregister_input_map(TARGET_INPUT_MAP, self.target_callback);
        }
    }

    #[must_use]
    pub const fn target_input_map_active(&self) -> bool {
        self.target_input_map_active
    }

    // ---------------------------------------------------------------------------------------
    // Rebinding a key, and making it survive the session
    // ---------------------------------------------------------------------------------------

    /// The configured keymap filename resolved to a path, or `None` when the client would not save.
    #[must_use]
    pub fn keymap_path(&self) -> Option<&std::path::Path> {
        self.keymap_path.as_deref()
    }

    /// The keymap basename shown by the keyboard options panel.
    #[must_use]
    pub fn keymap_file_name(&self) -> Option<String> {
        self.keymap_path
            .as_deref()
            .and_then(std::path::Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
    }

    /// The keyboard page's label, without changing the file used for persistence.
    #[must_use]
    pub fn keymap_display_name(&self) -> Option<String> {
        let file = self
            .keymap_file_name()
            .unwrap_or_else(|| DEFAULT_KEYMAP_FILE.into());
        if file.eq_ignore_ascii_case(DEFAULT_KEYMAP_FILE)
            || file.eq_ignore_ascii_case("acclient.keymap")
        {
            Some("Default".into())
        } else {
            scheme_name(&file, MODERN_SLUG).or(Some(file))
        }
    }

    /// The names of one interface's saved key maps (`<name>-<slug>.keymap`) in the folder,
    /// sorted.
    pub fn scheme_names(&self, slug: &str) -> Result<Vec<String>, dereth_input::InputError> {
        Ok(self
            .keymap_files()?
            .iter()
            .filter_map(|f| scheme_name(f, slug))
            .collect())
    }

    /// The name of the modern interface's key map in use, when its file is one of the scheme's.
    #[must_use]
    pub fn modern_scheme_in_use(&self) -> Option<String> {
        self.keymap_file_name()
            .and_then(|f| scheme_name(&f, MODERN_SLUG))
    }

    /// Every `*.keymap` in the folder, by basename.
    pub fn keymap_files(&self) -> Result<Vec<String>, dereth_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(Vec::new());
        };
        let mut names = Vec::new();
        for path in dereth_client_runtime::platform::files::list(dir)? {
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("keymap"))
            {
                if let Some(name) = path.file_name() {
                    names.push(name.to_string_lossy().into_owned());
                }
            }
        }
        names.sort_by_key(|name| name.to_ascii_lowercase());
        Ok(names)
    }

    /// Initialize the keymap from the selected file when the load dialog closes.
    /// A malformed user file deliberately becomes the two shipped defaults: keymap
    /// initialization clears the partial user map after adding it fails and then adds
    /// the game keymap and the base keymap, in that order.
    pub fn load_keymap_file(&mut self, name: &str) -> Result<bool, dereth_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(false);
        };
        let Some(file_name) = std::path::Path::new(name).file_name() else {
            return Ok(false);
        };
        if file_name != std::path::Path::new(name) {
            return Ok(false);
        }
        let path = dir.join(file_name);
        let text = dereth_client_runtime::platform::files::read_to_string(&path)?;
        let user = dereth_input::MasterInputMap::from_keymap_text(&text).ok();
        let Some(maps) = self.manager.shipped_maps.take() else {
            return Err(dereth_input::InputError::KeymapFile(
                "the shipped keymaps are unavailable".to_owned(),
            ));
        };
        let merged = dereth_input::scheme::over_defaults(
            user.as_ref(),
            &[&maps.0, &maps.1],
            Some(&self.manager.action_map),
        );
        *self.modern_map_mut() = merged;
        self.manager.shipped_maps = Some(maps);
        self.keymap_path = Some(path);
        Ok(true)
    }

    // ---------------------------------------------------------------------------------------
    // The classic interface's key map
    // ---------------------------------------------------------------------------------------

    /// The two shipped key maps, the game's and the engine's, in the order they are read.
    fn shipped_maps(&self) -> Vec<&dereth_input::MasterInputMap> {
        self.manager
            .shipped_maps
            .as_ref()
            .map(|m| vec![&m.0, &m.1])
            .unwrap_or_default()
    }

    /// The classic interface's default scheme, as a key map over the same keyboard as the retail
    /// one.
    #[must_use]
    pub fn classic_defaults(&self) -> dereth_input::MasterInputMap {
        dereth_classic_ui::default_keys::default_map(
            self.modern_map(),
            &self.manager.action_map,
            &self.shipped_maps(),
        )
    }

    /// The classic interface's key map, read the first time it is wanted: its file over its
    /// default scheme, the default scheme alone when there is no file yet.
    pub fn classic_keymap(&mut self) -> &mut ClassicKeymap {
        if self.classic.is_none() {
            let defaults = self.classic_defaults();
            let path = self
                .keymap_path
                .as_deref()
                .and_then(std::path::Path::parent)
                .map(|dir| dir.join(CLASSIC_KEYMAP_FILE));
            let file = path
                .as_ref()
                .and_then(|p| dereth_client_runtime::platform::files::read_to_string(p).ok())
                .and_then(|text| dereth_input::MasterInputMap::from_keymap_text(&text).ok());
            let map = dereth_input::scheme::over_defaults(
                file.as_ref(),
                &[&defaults],
                Some(&self.manager.action_map),
            );
            self.classic = Some(ClassicKeymap {
                map,
                defaults,
                path,
            });
        }
        if self.classic_active {
            self.classic.as_mut().expect("active map").map = self.manager.keymap.clone();
        }
        self.classic.as_mut().expect("just made")
    }

    /// The classic key map as the classic interface reads it.
    pub fn classic_keys(&mut self) -> dereth_classic_ui::keystore::ClassicKeys {
        use dereth_classic_ui::keystore::{ClassicBinding, ClassicKeys};
        let active = scheme_name(CLASSIC_KEYMAP_FILE, CLASSIC_SLUG).unwrap_or_default();
        let files = self
            .scheme_names(CLASSIC_SLUG)
            .unwrap_or_default()
            .into_iter()
            .filter(|n| !n.eq_ignore_ascii_case(&active))
            .collect();
        let mut maps: Vec<u32> = dereth_input::presentation::ROWS
            .iter()
            .map(|r| r.map)
            .collect();
        maps.sort_unstable();
        maps.dedup();
        let conflicts = maps
            .iter()
            .map(|m| {
                (
                    *m,
                    self.manager
                        .action_map
                        .conflicting_input_maps(InputMapId(*m))
                        .iter()
                        .map(|x| x.0)
                        .collect(),
                )
            })
            .collect();
        let holds = dereth_input::presentation::ROWS
            .iter()
            .filter(|r| {
                self.manager
                    .action_map
                    .toggle_type(r.input_map(), r.action())
                    .is_hold()
            })
            .map(|r| (r.map, r.action().0))
            .collect();
        let classic = self.classic_keymap();
        // The keys of the rows the key pages list.
        let bindings = dereth_input::scheme::keyboard_bindings(&classic.map)
            .filter(|(map, _, action)| dereth_input::presentation::find(*map, *action).is_some())
            .filter_map(|(map, qc, action)| {
                Some(ClassicBinding {
                    scan: qc.control.offset(),
                    modifiers: dereth_classic_ui::keystore::modifiers_of_meta(qc.meta_mode)?,
                    action: action.0,
                    map: map.0,
                })
            })
            .collect();
        ClassicKeys {
            bindings,
            files,
            active,
            conflicts,
            holds,
        }
    }

    /// The keyboard key `scan` held with `modifiers`, as the shipped maps bind keyboard keys.
    fn classic_key(&self, scan: u16, modifiers: u8) -> Option<dereth_input::ControlChord> {
        dereth_input::scheme::keyboard_key(
            &self.manager.keymap,
            scan,
            dereth_classic_ui::keystore::meta_of_modifiers(modifiers),
        )
    }

    /// The classic scheme `scheme` as a whole key map: the default scheme, or one of this
    /// interface's saved key maps.
    fn scheme(
        &mut self,
        scheme: &dereth_classic_ui::keystore::Scheme,
    ) -> Option<dereth_input::MasterInputMap> {
        use dereth_classic_ui::keystore::Scheme;
        match scheme {
            Scheme::Default => Some(self.classic_defaults()),
            Scheme::File(name) => {
                let dir = self.keymap_path.as_deref()?.parent()?;
                let file = dir.join(scheme_file(name, CLASSIC_SLUG));
                let text = dereth_client_runtime::platform::files::read_to_string(&file).ok()?;
                dereth_input::MasterInputMap::from_keymap_text(&text).ok()
            }
        }
    }

    /// Carry out one of the classic key page's requests on the classic key map, and write it.
    pub fn classic_request(&mut self, request: dereth_classic_ui::keystore::KeyStoreRequest) {
        use dereth_classic_ui::keystore::KeyStoreRequest as R;
        let result = match request {
            R::Bind {
                scan,
                modifiers,
                action,
                map,
                replaced,
            } => {
                let key = self.classic_key(scan, modifiers);
                let old = replaced.and_then(|(s, m)| self.classic_key(s, m));
                let action_map = self.manager.action_map.clone();
                let classic = self.classic_keymap();
                if let Some(key) = key {
                    dereth_input::scheme::bind(
                        &mut classic.map,
                        &action_map,
                        InputMapId(map),
                        key,
                        ActionId(action),
                        old,
                    );
                }
                classic.save()
            }
            R::Clear {
                scan,
                modifiers,
                action,
                map,
            } => {
                let key = self.classic_key(scan, modifiers);
                let classic = self.classic_keymap();
                if let Some(key) = key {
                    dereth_input::scheme::clear(
                        &mut classic.map,
                        InputMapId(map),
                        key,
                        ActionId(action),
                    );
                }
                classic.save()
            }
            R::SaveAs { name, overwrite } => self.save_classic_as(&name, overwrite),
            R::Load(scheme) => match self.scheme(&scheme) {
                Some(scheme) => {
                    let action_map = self.manager.action_map.clone();
                    let classic = self.classic_keymap();
                    let file = dereth_input::scheme::exactly(&scheme, &[&classic.defaults]);
                    classic.map = dereth_input::scheme::over_defaults(
                        Some(&file),
                        &[&classic.defaults],
                        Some(&action_map),
                    );
                    classic.save()
                }
                None => Err(dereth_input::InputError::KeymapFile(format!(
                    "the key scheme {scheme:?} could not be read"
                ))),
            },
            R::Delete(name) => {
                let file = scheme_file(&name, CLASSIC_SLUG);
                if file.eq_ignore_ascii_case(CLASSIC_KEYMAP_FILE) {
                    Ok(())
                } else {
                    self.delete_keymap_file(&file).map(|_| ())
                }
            }
        };
        if self.classic_active {
            if let Some(classic) = &self.classic {
                self.manager.keymap = classic.map.clone();
            }
        }
        if let Err(e) = result {
            tracing::warn!("the classic key map: {e}");
        }
    }

    /// Write the classic key map as `name`, the file `<name>-classic.keymap` beside the others,
    /// replacing one of that name only when `overwrite`. The classic key map in use is not
    /// written this way.
    fn save_classic_as(
        &mut self,
        name: &str,
        overwrite: bool,
    ) -> Result<(), dereth_input::InputError> {
        let file = scheme_file(name, CLASSIC_SLUG);
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf)
        else {
            return Ok(());
        };
        if file.eq_ignore_ascii_case(CLASSIC_KEYMAP_FILE)
            || name.trim().is_empty()
            || std::path::Path::new(&file).file_name() != Some(file.as_ref())
        {
            return Ok(());
        }
        let path = dir.join(file);
        if dereth_client_runtime::platform::files::exists(&path) && !overwrite {
            return Ok(());
        }
        let text = self.classic_keymap().map.to_keymap_text();
        dereth_client_runtime::platform::files::write(&path, text)?;
        Ok(())
    }

    /// Back to the shipped defaults: the player's own keys are dropped.
    pub fn restore_shipped_keys(&mut self) -> bool {
        let Some((game, base)) = self.manager.shipped_maps.as_deref() else {
            return false;
        };
        let restored = dereth_input::scheme::over_defaults(
            None,
            &[game, base],
            Some(&self.manager.action_map),
        );
        *self.modern_map_mut() = restored;
        true
    }

    /// Delete the key map file `name` (a basename, `.keymap` added when absent) beside the one in
    /// use. The one in use is not deleted.
    pub fn delete_keymap_file(&mut self, name: &str) -> Result<bool, dereth_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(false);
        };
        let mut file = std::path::PathBuf::from(name.trim());
        if file.file_name() != Some(file.as_os_str()) {
            return Ok(false);
        }
        if file
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("keymap"))
        {
            file = std::path::PathBuf::from(format!("{}.keymap", name.trim()));
        }
        let path = dir.join(file);
        if self.keymap_path.as_deref() == Some(path.as_path()) {
            return Ok(false);
        }
        dereth_client_runtime::platform::files::remove_file(&path)?;
        Ok(true)
    }

    /// `SaveKeymap` for a new name from the Save Keymap dialog: the modern interface's key map
    /// `name`, the file `<name>-modern.keymap`.
    pub fn save_keymap_as(
        &mut self,
        name: &str,
        overwrite: bool,
    ) -> Result<Option<SaveKeymapAs>, dereth_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(None);
        };
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        let file_name = std::path::PathBuf::from(scheme_file(name, MODERN_SLUG));
        if file_name.file_name().is_none() || file_name.file_name() != Some(file_name.as_os_str()) {
            return Ok(None);
        }
        let path = dir.join(file_name);
        if dereth_client_runtime::platform::files::exists(&path) {
            if dereth_client_runtime::platform::files::read_only(&path)? {
                return Ok(Some(SaveKeymapAs::ReadOnly));
            }
            if !overwrite {
                return Ok(Some(SaveKeymapAs::NeedsOverwrite));
            }
        }
        if let Err(error) = write_keymap(self.modern_map(), &path) {
            if matches!(&error, dereth_input::InputError::Io(io) if io.kind() == std::io::ErrorKind::PermissionDenied)
            {
                return Ok(Some(SaveKeymapAs::ReadOnly));
            }
            return Err(error);
        }
        self.keymap_path = Some(path);
        Ok(Some(SaveKeymapAs::Saved))
    }

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

    /// Save the keymap during cleanup when its filename is nonempty and it was loaded.
    /// The destination joins the settings directory and that filename.
    ///
    /// Saving does not require the preference `Input.KeymapFile` to be set. Initialization assigns
    /// the keymap-file path either from the preference or, when the preference is empty, from
    /// the running executable's file name with the extension changed to `keymap` — so
    /// the path is **never** empty after successful keymap initialization, and a retail client
    /// therefore writes `<prefs dir>\acclient.keymap` on every clean exit. That is why no shipped
    /// install carries a `.keymap` and every played one does.
    ///
    /// What is written is the **full merged map**, defaults included, not just the overrides
    /// — [`dereth_input::InputManager::save_keymap`] is the writer and this is its caller.
    ///
    /// Returns `Ok(false)` when there is no path, which is the client's own skip.
    ///
    /// # Errors
    /// [`dereth_input::InputError::Io`] if the file cannot be written.
    pub fn save_keymap(&self) -> Result<bool, dereth_input::InputError> {
        let Some(path) = self.keymap_path.as_ref() else {
            return Ok(false);
        };
        write_keymap(self.modern_map(), path)?;
        Ok(true)
    }

    /// The input maps a **focused** text
    /// element owns, registered while it has focus and unregistered when it loses it.
    ///
    /// **Without these, Enter does nothing in a text box.** [`BASE_MAP_REGISTRATIONS`] is
    /// the startup subset of [`dereth_input::RETAIL_MAP_REGISTRATIONS`], and the
    /// rows it leaves out are the ones a *focused* element adds:
    ///
    /// ```text
    /// (scrollable element, 0x0A, FOCUSED_UI)       // ScrollableControls — the mouse wheel
    /// (text element,          1, FOCUSED_UI)       // MAP_BLOCK_KEYBOARD, the typing barrier
    /// (text element,          7, FOCUSED_UI)       // the text-editing map
    /// (text element,          8, FOCUSED_UI)       // …and its second half
    /// ```
    ///
    /// **`0x0A` is what makes the mouse wheel work.** See [`FOCUSED_TEXT_MAPS`] for the two retail
    /// registration rules and for the walk order. The scope this build implements is stated
    /// there: `0x0A` rides the *text*
    /// element's focus hook, which is the only focus-driven registration this shell has, so a
    /// focused `ListBox` — a scrollable in retail too — does not yet push it.
    ///
    /// Maps 7 and 8 are where the text actions `0x16`…`0x28` live — the arrows, Home/End, PageUp,
    /// Copy/Cut/Paste, Delete, Backspace, **Escape `0x27` and Enter `0x25`**. With neither of them
    /// registered, `walk_input_maps` finds no binding for a keystroke in a text box, no
    /// `InputEvent` is produced at all, and every one of those keys is inert however good the
    /// dispatch above it is: a `WM_KEYDOWN` of `VK_RETURN` while the chat entry holds focus
    /// produces no key press.
    ///
    /// **The barrier (map 1) is registered here too, so that typing in the chat box does not walk
    /// the character.** `walk_input_maps` `break`s on
    /// [`dereth_input::MAP_BLOCK_KEYBOARD`] for any control on the **keyboard** device and skips it
    /// for everything else, and that is the *entire* mechanism by which the client stops a typed
    /// `w` also reaching the player movement map: there is no "does the UI want this key?"
    /// test anywhere in `fire_input_event`. Mouse controls pass the barrier, which is why you can
    /// still click-to-select in the world while the chat bar has focus.
    ///
    /// It must end up **after** 7 and 8 in the walk or it blocks them too, and it does, because
    /// sorted list insertion puts an equal-priority newcomer in **front**: registering
    /// `1, 7, 8` — the client's own order — yields the walk order `8, 7, 1`. A `register` that
    /// appended within a band would make 7 and 8 unreachable.
    /// They are registered under a **callback of their own**, not the client's, and the
    /// removal unregisters that callback rather than individual input maps.
    /// That is not tidiness — it prevents a hang:
    ///
    /// The Enter-key handler drops the entry's focus **inside the key-down**, so the maps are gone by
    /// the time the matching key-**up** arrives; the up then resolves to no action at all, the
    /// `ActionState` for `0x25` is never released, and the input manager's repeat sweep
    /// re-fires Enter **every frame for ever**. `unregister_callback` is the one entry point that
    /// also drops that callback's `ActionState`s — dereth-input's own note on it is "so a destroyed
    /// panel cannot leave a held action behind", and a text element that has just lost focus is
    /// exactly that panel.
    /// **Which maps a focused element pushes is a property of the element** — `0x0A` for every
    /// scrollable, 1 and 7 only for an editable text box, 8 for an editable-or-selectable one — so
    /// it takes a set: a single "an editable text box has focus" flag would leave a focused list
    /// box or a selectable chat log registering nothing, with a dead wheel. The set is computed by
    /// `dereth_client::ui::UiShell::focused_input_maps`, which carries both registration rules.
    ///
    /// An empty slice is the focus change's lose arm. A non-empty one is the
    /// lose arm **and then** the gain arm, in that order, because the callback is dropped whole:
    /// `unregister_callback` discards the callback's held states so a retired widget cannot
    /// keep repeating an editing command.
    /// **It takes `&[(u32, i32)]` rather than `&[u32]`** because text registration
    /// chains through the scrollable base to the base element's registration. This registers
    /// the element's own input map at the
    /// priority it was handed and each **ancestor**'s one lower. A `map -> priority` function
    /// cannot express that, because the same map id at two depths goes in at two priorities.
    /// [`FOCUSED_TEXT_MAP_REGISTRATIONS`] is the constant for the four that are uniform.
    pub fn set_focused_input_maps(&mut self, maps: &[(u32, i32)]) {
        if self.focused_maps == maps {
            return;
        }
        self.focused_maps = maps.to_vec();
        self.manager.unregister_callback(self.text_callback);
        for (map, prio) in maps {
            self.manager
                .register_input_map(InputMapId(*map), *prio, self.text_callback);
        }
    }

    /// **The `priority::UNFOCUSED_UI` band: what the newly *active* root element registers.**
    ///
    /// Element activation makes two map-registration calls on the window it
    /// activates and this mirrors both, in the client's own order:
    ///
    /// First register at priority 0. Then issue the activation alert, which registers
    /// the active element's maps at priority 2000.
    ///
    /// Deactivation makes one map-unregistration call that takes both away — see
    /// [`dereth_input::dispatch::InputMapStack::unregister`], which removes **all** priorities of a
    /// `{map, callback}` pair so this round-trips.
    ///
    /// `maps` is `(map, priority)` in registration order, from
    /// `dereth_ui::UiSystem::input_maps_for_registration`; an empty slice is the deactivation.
    /// Every entry goes in under [`Self::active_callback`], because in the client every one of them
    /// is registered with the *element* as the input-action callback.
    ///
    /// **With the shipped layouts this is always empty**, and that is a measurement rather than an
    /// assumption: the element input map (attribute `0x4E`) is set on 2 of 2 162 elements and
    /// neither is a root element. The mechanism, the census and the calibration are on
    /// `dereth_ui::UiSystem::input_maps_for_registration`.
    pub fn set_active_input_maps(&mut self, maps: &[(u32, i32)]) {
        self.manager.unregister_callback(self.active_callback);
        for (map, prio) in maps {
            self.manager
                .register_input_map(InputMapId(*map), *prio, self.active_callback);
        }
    }

    /// The callback the active root element's maps are registered under.
    #[must_use]
    pub const fn active_callback(&self) -> CallbackId {
        self.active_callback
    }

    /// **The maps registered for the lifetime of the current pre-game screen.** The screen
    /// registers them when it is constructed and removes them when it is destroyed; without them
    /// Enter reaches no `AcceptInput`.
    ///
    /// Three original constructors do this and there is **no fourth**. Each obtains the input
    /// manager, null-checks it, then registers the screen as a map callback at priority `0xBB8`:
    ///
    /// Credits, character management and intro each register map 9 at priority 3000, and intro
    /// registers map 3 at 3000 too.
    ///
    ///
    /// Map **9** is `DialogBoxes`, and the shipped merged keymap gives it exactly two controls —
    /// `DIK_ESCAPE` → `EscapeKey` and `DIK_RETURN` → `AcceptInput`. Without this registration
    /// neither control has a registered map to be found in while a pre-game screen is up, so
    /// `walk_input_maps` would resolve `DIK_RETURN` in `ChatCommands` (`0x1000000A`,
    /// `priority::GAMEPLAY`) to `EnterChatMode` instead, which no pre-game screen answers.
    ///
    /// Registered under [`Self::mode_callback`] and **not** the client's, for the same reason the
    /// focused and active bands have their own: `unregister_callback` is what also drops
    /// the callback's held `ActionState`s, and a screen that has just been destroyed is exactly the
    /// object whose held actions must go with it.
    ///
    /// An empty slice is the destructor.
    pub fn set_mode_input_maps(&mut self, maps: &[u32]) {
        if self.mode_maps == maps {
            return;
        }
        self.mode_maps = maps.to_vec();
        self.manager.unregister_callback(self.mode_callback);
        for map in maps {
            self.manager.register_input_map(
                InputMapId(*map),
                dereth_input::dispatch::priority::FOCUSED_UI,
                self.mode_callback,
            );
        }
    }

    /// The callback the current pre-game screen's maps are registered under.
    #[must_use]
    pub const fn mode_callback(&self) -> CallbackId {
        self.mode_callback
    }

    /// Swap the combat-mode input map.
    ///
    /// Three steps, in the client's own order, with the two arguments the way the combat-mode
    /// setter passes them (the new combat mode first, then the old):
    ///
    /// 1. Unregister `map_for(old)` from the callback — only when `old` is one of the three
    ///    fighting modes; peace and undefined fall off the ladder and unregister nothing.
    /// 2. Register `0x10000002` under the callback at priority 1000 — unconditional, and inert after the
    ///    first time, because the constructor already registered that exact triple. It is written
    ///    out rather than elided so the transcription matches the function.
    /// 3. Register `map_for(new)` under the callback at priority 1000 — again only for a fighting mode.
    ///
    /// Step 3 is what puts the live combat map at the **front** of the `priority::GAMEPLAY` band:
    /// [`dereth_input::dispatch::InputMapStack::register`] inserts an equal-priority newcomer ahead
    /// of what is there, matching the client's sorted-list insertion. That ordering is load-bearing in
    /// exactly one place and it is not the combat keys: `MagicCombat` binds `DIK_1`…`DIK_9` to
    /// `UseSpellSlot_1`…`_9` and `QuickslotCommands` binds the same nine keys to
    /// `UseQuickSlot_1`…`_9`, so **in magic combat mode the number row casts spells and in every
    /// other mode it uses quickslots**. The shipped action-map conflict table agrees from
    /// a completely different direction: it is the only pair of *simultaneously registered* maps
    /// in the shipped data that shares controls and is declared not to conflict.
    ///
    /// `mode` values are `COMBAT_MODE` (`dereth_input::combat::mode`), which is what
    /// `dereth_client_model::CombatMode::raw` answers.
    pub fn register_combat_input_maps(&mut self, new_mode: u32, old_mode: u32) {
        use dereth_input::combat::{input_map_for_combat_mode, COMBAT_MAP};
        if let Some(old) = input_map_for_combat_mode(old_mode) {
            self.manager.unregister_input_map(old, self.combat_callback);
        }
        self.manager.register_input_map(
            COMBAT_MAP,
            dereth_input::dispatch::priority::GAMEPLAY,
            self.combat_callback,
        );
        if let Some(new) = input_map_for_combat_mode(new_mode) {
            self.manager.register_input_map(
                new,
                dereth_input::dispatch::priority::GAMEPLAY,
                self.combat_callback,
            );
        }
    }

    /// The callback under which the combat subsystem's four input maps are registered.
    #[must_use]
    pub const fn combat_callback(&self) -> CallbackId {
        self.combat_callback
    }

    /// Update combat-mode input registrations, driven from the mode
    /// this build already keeps in `dereth_client_model::CombatState::combat_mode`.
    ///
    /// The client's combat-mode setter early-returns when the mode is unchanged and otherwise calls
    /// [`Self::register_combat_input_maps`] with the new mode and the old one, so the shell keeps
    /// the old mode rather than making every caller carry it. Returns whether anything moved,
    /// which is what a test asserts on.
    ///
    /// Return early when the requested
    /// mode equals the current combat mode. The later compatibility check is a separate
    /// branch. The transition notification represented here receives both the saved old mode
    /// and the new mode, which has already been assigned before the call.
    ///
    /// The production caller is
    /// `dereth_client::interaction::Interaction::update_combat_input_map`, which polls once per frame.
    ///
    /// Outside a character session there is no combat mode to follow: the combat maps are down
    /// (see [`Self::set_character_session_input_maps`]) and this changes nothing.
    pub fn set_combat_input_maps(&mut self, mode: u32) -> bool {
        if !self.character_session_input_maps || mode == self.combat_input_mode {
            return false;
        }
        let old = std::mem::replace(&mut self.combat_input_mode, mode);
        self.register_combat_input_maps(mode, old);
        true
    }

    /// The `COMBAT_MODE` the combat input maps are currently registered for.
    #[must_use]
    pub const fn combat_input_mode(&self) -> u32 {
        self.combat_input_mode
    }

    /// **Begin or end the character session's input maps.** Returns whether anything moved.
    ///
    /// Beginning a session registers every row of [`BASE_MAP_REGISTRATIONS`] outside
    /// [`WHOLE_RUN_INPUT_MAPS`] — movement and emotes, item selection and the target cycle, the UI
    /// commands (`E` examine, `R` use, the panel toggles), the quickbar, `Combat` and the chat keys — in the table's
    /// own order, so the walk comes back exactly as the constructor built it: every one of them is
    /// at or above `priority::GAMEPLAY`, and an equal-priority newcomer goes in front of the camera
    /// map that stayed.
    ///
    /// Ending it queues releases before the session's callbacks are retired by
    /// [`Self::finish_session_retirement`], taking the combat and target maps with them. Until the next session
    /// begins, combat-mode and target-mode changes register nothing.
    ///
    /// The production caller is `dereth_client::ui::UiShell`, which mirrors "the gameplay screen
    /// is up" into this on the edge, as it mirrors a pre-game screen's own maps.
    pub fn set_character_session_input_maps(&mut self, live: bool) -> bool {
        if self.character_session_input_maps == live {
            return false;
        }
        if live {
            self.character_session_input_maps = true;
            if self.retiring_session {
                return true;
            }
            self.combat_input_mode = dereth_input::combat::mode::NONCOMBAT;
            for (owner, map, prio) in BASE_MAP_REGISTRATIONS {
                if WHOLE_RUN_INPUT_MAPS.contains(map) {
                    continue;
                }
                let cb = if *owner == "combat" {
                    self.combat_callback
                } else {
                    self.session_callback
                };
                self.manager.register_input_map(InputMapId(*map), *prio, cb);
            }
        } else {
            self.manager.release_pressed_keys();
            self.retiring_actions
                .extend(self.manager.take_events().iter().map(InputEvent::to_action));
            self.retiring_session = true;
            self.character_session_input_maps = false;
        }
        true
    }

    /// Whether the character-session maps are registered. See
    /// [`Self::set_character_session_input_maps`].
    #[must_use]
    pub const fn character_session_input_maps(&self) -> bool {
        self.character_session_input_maps
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
    pub fn on_message(&mut self, m: crate::pump::Win32Message) -> bool {
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
    /// arm; `UiSystem` has no route to the input manager, so `dereth_client::ui::UiShell::frame`
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
    use crate::platform::keys::Key;
    use dereth_client_runtime::actions::movement::action;
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
            .button_message(crate::platform::keys::MouseButton::Left, true, 1000)
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
        shell.on_message(crate::pump::Win32Message::new(
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
        shell.on_message(crate::pump::Win32Message::new(
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
        shell.on_message(crate::pump::Win32Message::new(
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
                shell.on_message(crate::pump::Win32Message::new(msg::WM_LBUTTONDOWN, 0, 0, t));
                shell.on_message(crate::pump::Win32Message::new(
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
        let m = |message, wparam| crate::pump::Win32Message::new(message, wparam, 0, 1);
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
        let move_msg = crate::pump::Win32Message::new(msg::WM_MOUSEMOVE, 0, 0x0064_0064, 2);
        assert!(shell.on_message(move_msg));
        assert_eq!(
            shell.manager.mouse_pos(),
            (100, 100),
            "the mouse position is stored, not dispatched"
        );
        assert_eq!(shell.stats.messages_handled, 2);
    }
}
