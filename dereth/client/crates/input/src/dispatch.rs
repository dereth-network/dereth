//! Dispatch: the handler stack, map registration and priorities, text mode and
//! the ignore-next-char latch.
//!
//! This covers the client's registering and unregistering of an input map, unregistering a
//! callback, registering an input handler, sending an action to the listeners, and the text-mode
//! switch and character update.

use crate::fire::InputMapEntry;
use crate::{CallbackId, InputMapId};

/// The retail input-map priorities.
pub mod priority {
    pub const LOWEST: i32 = 0;
    pub const GAMEPLAY: i32 = 1000;
    /// **The unfocused-UI priority, and the whole band has exactly three producers.** The client
    /// has exactly three input-map registrations at 2000:
    ///
    /// | site | what it registers |
    /// |---|---|
    /// | the activation alert | the newly **active** element registers its input maps at 2000 |
    /// | the camera system | map **6** `CameraAlternateControls`, while alternate camera mode is on |
    /// | the UI system, four sites | map **`0x1000000B`** `TargetedUsage`, while target mode is armed |
    ///
    /// Nothing registers at a literal 2990: the typing barrier is computed as `3000 - 10`.
    /// \[verified\]
    ///
    /// The second and third are wired (`App::apply_world_camera_action`, and the target-mode arm);
    /// the first goes through `UiShell::active_input_maps`.
    pub const UNFOCUSED_UI: i32 = 2000;
    pub const FOCUSED_UI: i32 = 3000;
    /// **2990 — the focused-UI priority minus ten, where the typing barrier really sits.**
    ///
    /// The focus change asks the gaining element to register its input maps at `3000`, and it
    /// registers maps 7 and 8 at that value but
    /// map **1** — `MAP_BLOCK_KEYBOARD`, the typing barrier — **ten below it**:
    ///
    /// ```text
    ///   register_input_map(map 1, priority - 10, callback)   // 2990 <-- the barrier
    ///   register_input_map(map 7, priority, callback)        // 3000
    ///   register_input_map(map 8, priority, callback)        // 3000
    /// ```
    ///
    /// (The base class, which runs first and unconditionally, registers map `0x0A` at the same
    /// priority.) \[verified\]
    ///
    /// **It changes the walk order**, which is the whole reason this constant is worth having.
    /// Registration prepends an equal-priority newcomer, so registering
    /// `0x0A, 1, 7, 8` with a uniform 3000 walks as `8, 7, 1, 0x0A` — the barrier third, blocking
    /// `ScrollableControls`' `Ctrl+DIK_UP`/`DIK_DOWN`. With map 1 ten lower it walks as
    /// `8, 7, 0x0A, 1`: the barrier is genuinely last, and that pair reaches `ScrollableControls`
    /// while a text box has focus.
    pub const FOCUSED_UI_TEXT_BARRIER: i32 = 2990;
    pub const DEBUG_CONSOLE: i32 = 4000;
    /// The client registers map 0x10 at −1, below the lowest priority (0), so the client's
    /// system-key swallow is always last.
    pub const CLIENT_SYSTEM_KEYS: i32 = -1;
    /// The chat bar registers `0x1000000D` here **on purpose**, above
    /// the focused-UI priority (3000), so Tab still leaves the chat bar.
    pub const TOGGLE_CHAT_ENTRY: i32 = 3010;
    /// The debug console registers the keyboard barrier at 3999 while its input
    /// line is active — one below its own maps at 4000.
    pub const DEBUG_CONSOLE_BARRIER: i32 = 3999;
}

/// The input-handler flag mask registration takes.
pub mod handler_flags {
    pub const ACTION: u32 = 0x01;
    pub const CHARACTER: u32 = 0x02;
    pub const MOUSE_LOOK: u32 = 0x04;
    pub const FOCUS_SWITCH: u32 = 0x08;
    pub const MOUSE_MOVE: u32 = 0x10;
    /// Exclusive: a single key-hit handler. The key-binding options page takes it while a
    /// binding is being captured, and it runs **before any input map is consulted**.
    pub const KEY_HIT: u32 = 0x20;
}

/// The priority-ordered stack `fire_input_event` walks.
#[derive(Debug, Default)]
pub struct InputMapStack {
    entries: Vec<InputMapEntry>,
}

impl InputMapStack {
    #[must_use]
    pub fn entries(&self) -> &[InputMapEntry] {
        &self.entries
    }

    /// Register an input map -- rejects an exact `{map_id, callback, priority}` and otherwise
    /// inserts **descending by
    /// priority**, so `fire_input_event` walks the highest priority first.
    ///
    /// The sorted insert puts an equal-priority newcomer **in front of** the entries already there,
    /// not after them. The direction is load-bearing: the focus change registers the keyboard
    /// barrier (map **1**) *first* and the two text maps 7 and 8 *after* it, all three at the
    /// focused-UI priority. Under "append within a band" that ordering puts the barrier in front of
    /// 7 and 8 and [`crate::fire::walk_input_maps`] breaks before it ever reaches them — every key
    /// in a focused text box inert. Under the client's own ordering the walk sees `8, 7, 1` and the
    /// barrier is last, which is what makes the barrier stop *movement* without stopping the text
    /// box.
    ///
    /// The walk:
    ///
    /// ```text
    ///   walk while (new priority < node priority)
    ///   insert the new node BEFORE the node the walk stopped on
    /// ```
    ///
    /// so the new node is linked **before** the node the walk stopped on. So the loop stops at the first
    /// node whose priority is `<=` the newcomer's, and the newcomer goes ahead of it. \[verified\]
    pub fn register(&mut self, map: InputMapId, priority: i32, callback: CallbackId) {
        let candidate = InputMapEntry {
            map,
            priority,
            callback,
        };
        if self.entries.contains(&candidate) {
            return;
        }
        let at = self
            .entries
            .iter()
            .position(|e| e.priority <= priority)
            .unwrap_or(self.entries.len());
        self.entries.insert(at, candidate);
    }

    /// Unregister an input map -- **every** entry with that
    /// `{map_id, callback}` pair, at any priority.
    ///
    /// Not just the *first* match: the client's loop does not stop, and it never looks at the
    /// priority:
    ///
    /// ```text
    ///   for each node: if (node.map == map_id && node.callback == callback) remove it
    ///                  and keep walking the whole list      ; no priority compare
    /// ```
    ///
    /// \[verified\] It only became observable when one element could hold the same map at two
    /// priorities, which is exactly what an element with an activation alert does: it registers
    /// its maps at priority 0 and again at 2000, both with
    /// the element as the callback. Registration rejects only an exact
    /// `{map, callback, priority}` triple (the duplicate test compares all three fields),
    /// so both entries exist — and a remove-the-first `unregister` would have
    /// left the lowest-priority copy behind for ever on every deactivation.
    pub fn unregister(&mut self, map: InputMapId, callback: CallbackId) {
        self.entries
            .retain(|e| !(e.map == map && e.callback == callback));
    }

    /// Unregister a callback -- every entry belonging to it. The
    /// caller must also drop that callback's `ActionState`s
    /// ([`crate::state::ActionStates::remove_by_callback`]), so a destroyed panel cannot leave a
    /// held action behind.
    pub fn unregister_callback(&mut self, callback: CallbackId) {
        self.entries.retain(|e| e.callback != callback);
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Text mode and the ignore-next-char latch.
///
/// Pressing Enter fires *Begin Chat Mode*; that handler focuses the chat field; the field
/// turns text mode on **while still inside the `WM_KEYDOWN`**; the ignore-next-char flag is set; and the
/// `WM_CHAR` that `TranslateMessage` produces for that same key press is dropped. Without it every
/// chat line starts with the activation character.
#[derive(Debug, Default)]
pub struct TextMode {
    /// Text mode is a single global flag, not per-field.
    pub text_mode: bool,
    /// Whether a key-down message is being processed; set for the duration of a `WM_KEYDOWN` /
    /// `WM_SYSKEYDOWN`.
    pub processing_key_down: bool,
    /// Whether an action is being processed in response to a key-down, latched around the send
    /// to the listeners.
    pub processing_action_in_response_to_key_down: bool,
    /// Drop the next character message.
    pub ignore_next_char: bool,
}

impl TextMode {
    /// The wrapper that makes the latch
    /// visible to the text-mode switch handler.
    pub fn begin_dispatch(&mut self) {
        self.processing_action_in_response_to_key_down = self.processing_key_down;
    }

    pub fn end_dispatch(&mut self) {
        self.processing_action_in_response_to_key_down = false;
    }

    /// Setting text mode, then the switch handler:
    /// `if (entering text mode && processing_action_in_response_to_key_down) ignore_next_char = true;`
    pub fn set_text_mode(&mut self, on: bool) {
        self.text_mode = on;
        if on && self.processing_action_in_response_to_key_down {
            self.ignore_next_char = true;
        }
    }

    /// The character update:
    /// ```text
    /// if (!ignore_next_char && text_mode && has_focus) call the character handler (ch);
    /// ignore_next_char = false;
    /// ```
    /// The latch is cleared **unconditionally**, so it only ever eats one character.
    pub fn take_character(&mut self, has_focus: bool) -> bool {
        let deliver = !self.ignore_next_char && self.text_mode && has_focus;
        self.ignore_next_char = false;
        deliver
    }

    /// The message handler's `WM_KEYDOWN`/`WM_SYSKEYDOWN` arm:
    /// `processing_key_down = true; ignore_next_char = false;`
    pub fn begin_key_down(&mut self) {
        self.processing_key_down = true;
        self.ignore_next_char = false;
    }

    pub fn end_key_down(&mut self) {
        self.processing_key_down = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered input pipeline §6 — descending by priority, duplicates rejected, and the
    /// client's map 0x10 at −1 is always last.
    #[test]
    fn the_stack_is_descending_and_rejects_duplicates() {
        let mut s = InputMapStack::default();
        let cb = CallbackId(1);
        s.register(InputMapId(0x10), priority::CLIENT_SYSTEM_KEYS, cb);
        s.register(InputMapId(4), priority::GAMEPLAY, cb);
        s.register(InputMapId(3), priority::LOWEST, cb);
        s.register(InputMapId(1), priority::FOCUSED_UI, cb);
        s.register(InputMapId(0x1000000D), priority::TOGGLE_CHAT_ENTRY, cb);
        s.register(InputMapId(4), priority::GAMEPLAY, cb); // exact duplicate

        let order: Vec<u32> = s.entries().iter().map(|e| e.map.0).collect();
        assert_eq!(order, vec![0x1000000D, 1, 4, 3, 0x10]);
        assert_eq!(s.len(), 5);
    }

    /// Oracle: the sorted insert stops at the first node whose priority is not greater, and
    /// links the new node **before** it -- an equal-priority
    /// newcomer goes in front. The direction is why a text element can
    /// register the keyboard barrier first and still have maps 7 and 8 reachable.
    ///
    /// The bidirectional half matters: an implementation that appended within a band would give
    /// `[1, 7, 8]` here, which is the shape that made every key in a focused text box inert.
    #[test]
    fn an_equal_priority_registration_goes_in_front_of_the_ones_already_there() {
        let mut s = InputMapStack::default();
        let cb = CallbackId(1);
        // The focus change, in its own order: 1, then 7, then 8.
        for map in [1u32, 7, 8] {
            s.register(InputMapId(map), priority::FOCUSED_UI, cb);
        }
        let order: Vec<u32> = s.entries().iter().map(|e| e.map.0).collect();
        assert_eq!(
            order,
            vec![8, 7, 1],
            "the barrier is walked LAST, not first"
        );
        // …and the band as a whole still sits under a higher priority and over a lower one.
        s.register(InputMapId(0x1000_000D), priority::TOGGLE_CHAT_ENTRY, cb);
        s.register(InputMapId(4), priority::GAMEPLAY, cb);
        let order: Vec<u32> = s.entries().iter().map(|e| e.map.0).collect();
        assert_eq!(order, vec![0x1000_000D, 8, 7, 1, 4]);
    }

    /// **One `{map, callback}` pair can be in the stack at two priorities, and unregistering it
    /// takes both.**
    ///
    /// Oracle: registering an input map looks the triple up first, comparing map, callback
    /// **and** priority, so only an exact triple is a duplicate; and unregistering an input map
    /// compares only map and callback
    /// and **continues after a removal**. \[verified\]
    /// and **continues after a removal** (` eb e2`). \[verified\]
    ///
    /// This is not hypothetical: window activation registers the activated window's
    /// maps at the lowest priority (0) and registers the
    /// same maps under the same callback at the unfocused-UI priority (2000), and
    /// deactivation's single unregister of the window's maps has to clear both. A remove-the-first
    /// implementation leaves the lowest-priority copy behind on every deactivation, for ever.
    #[test]
    fn one_map_can_be_registered_at_two_priorities_and_unregistering_removes_both() {
        let mut s = InputMapStack::default();
        let win = CallbackId(1);
        let other = CallbackId(2);
        // The activation pair, in the client's own order.
        s.register(InputMapId(9), priority::LOWEST, win);
        s.register(InputMapId(9), priority::UNFOCUSED_UI, win);
        // A third registrant of the same map, which must survive.
        s.register(InputMapId(9), priority::GAMEPLAY, other);
        assert_eq!(
            s.len(),
            3,
            "the triple is {{map, callback, priority}}, so none of these is a \
             duplicate of another"
        );
        let order: Vec<i32> = s.entries().iter().map(|e| e.priority).collect();
        assert_eq!(
            order,
            vec![2000, 1000, 0],
            "descending, and the 2000 copy outranks gameplay"
        );

        s.unregister(InputMapId(9), win);
        assert_eq!(
            s.len(),
            1,
            "unregistering an input map walks the whole list and never compares the priority, so BOTH of the \
             window's entries go"
        );
        assert_eq!(
            s.entries()[0].callback,
            other,
            "and only the other owner's survives"
        );
    }

    /// Oracle: the recovered input pipeline §6 — `unregister_callback` removes every entry a callback owns.
    #[test]
    fn unregister_callback_removes_all_of_one_owners_maps() {
        let mut s = InputMapStack::default();
        let a = CallbackId(1);
        let b = CallbackId(2);
        s.register(InputMapId(7), priority::FOCUSED_UI, a);
        s.register(InputMapId(8), priority::FOCUSED_UI, a);
        s.register(InputMapId(5), priority::GAMEPLAY, b);
        s.unregister_callback(a);
        assert_eq!(s.len(), 1);
        assert_eq!(s.entries()[0].map, InputMapId(5));
    }

    /// Oracle: trap 6 and the recovered input pipeline §9 — the key that opened a text field is not typed
    /// into it, and the latch eats exactly one character.
    #[test]
    fn ignore_next_char_eats_exactly_the_activation_character() {
        let mut t = TextMode::default();
        // WM_KEYDOWN for Return arrives.
        t.begin_key_down();
        // fire_input_event resolves Begin Chat Mode and dispatches it ...
        t.begin_dispatch();
        // .. the handler focuses the chat field, which turns text mode on, still inside the down.
        t.set_text_mode(true);
        t.end_dispatch();
        t.end_key_down();
        assert!(t.ignore_next_char);
        // TranslateMessage produces WM_CHAR for the same key press: dropped.
        assert!(!t.take_character(true));
        // The next character is typed normally.
        assert!(t.take_character(true));
    }

    /// Oracle: the recovered input pipeline §9 — text mode alone is not enough; the window must have
    /// focus, and characters never reach the handler list outside text mode.
    #[test]
    fn characters_need_text_mode_and_focus() {
        let mut t = TextMode::default();
        assert!(!t.take_character(true), "not in text mode");
        t.set_text_mode(true);
        assert!(!t.take_character(false), "no window focus");
        assert!(t.take_character(true));
    }

    /// Oracle: the recovered input pipeline §9 — a text field focused by a *mouse* click sets no latch,
    /// because no action was dispatched in response to a key down.
    #[test]
    fn a_mouse_click_focusing_a_field_sets_no_latch() {
        let mut t = TextMode::default();
        t.begin_dispatch(); // processing_key_down is false
        t.set_text_mode(true);
        assert!(!t.ignore_next_char);
    }
}
