//! Input-map registrations scoped to focus, screens and character sessions.

use super::InputShell;
use dereth_input::{CallbackId, InputEvent, InputMapId};

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

impl InputShell {
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
}
