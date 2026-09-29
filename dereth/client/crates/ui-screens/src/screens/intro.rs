//! `IntroScreen` — mode `0x10000001`, the intro splash sequence.
//!
//! The screen's state list is filled from element attribute **`0x10000047`** on the root element
//! `0x10000419`. It is a `BasePropertyType::Array` whose members all carry the property
//! name **`0x10000048`** and an `Enum` value, and in the shipped `intro` layout (`0x21000001`) it
//! holds four state ids in this order:
//!
//! | # | state | what the layout plays in it |
//! |--:|---|---|
//! | 0 | `0x10000038` | movie media entry `turbine_logo_ac.avi` on child `0x10000434` |
//! | 1 | `0x10000039` | pause 0.1, image `0x0600610F`, fade 0→1 over 0.75, pause 2.75, fade 1→0 over 0.5 |
//! | 2 | `0x1000003A` | the same with image `0x06006EBC` |
//! | 3 | `0x1000003E` | movie media entry `AC-ThroneOfDestiny.avi` on child `0x10000444` — **not shipped**, so the step is skipped silently |
//!
//! Every one of the four state media lists **ends in a message media entry carrying `0x1000000D`**,
//! which is exactly the element message the screen's handler turns into `next_frame`.
//! So the list drives the states, and each state's own media list drives the advance.
//!
//! Read out of `client_local_English.dat` object `0x21000001` with
//! the table decoder, which consumes the payload to its exact end
//! (861 of 861 bytes).

use dereth_ui::framework::ScreenCx;
use dereth_ui::framework::{LayoutEnum, Screen};
use dereth_ui::{
    ElemHandle, ElementId, ElementMessage, ListenerId, MessageId, StateId, UiError, UiMode,
    UiSystem,
};

/// The root is created from layout enum `0x10000002`, element `0x10000419` — the intro field.
const LAYOUT: LayoutEnum = LayoutEnum(0x1000_0002);
const ROOT: ElementId = ElementId(0x1000_0419);
/// The framework's own listener identity, as creating the root by DataID registers
/// it on the root.
const ME: ListenerId = ListenerId::External(LAYOUT.0);

/// Element message `0x1000000D` — the media sequence finished; the element-message handler
/// calls `next_frame`.
pub const MSG_MEDIA_FINISHED: MessageId = MessageId(0x1000_000D);
/// Element message `0x10000001` — skip straight to character management.
///
/// **Nothing in the shipped layout raises it**. `0x21000001` holds exactly three
/// elements — the root `0x10000419` and the two 640x480 media fields `0x10000434` and
/// `0x10000444` — and none of them is a button, a hotspot, or carries a message media entry
/// with this id: the only message the four media lists post is `0x1000000D`. So the arm
/// the element-message handler keeps for it is **dead in this build**, and the click the
/// player makes is not an element message at all. It is [`IntroScreen::on_action`] — see
/// [`INPUT_MAPS`].
pub const MSG_SKIP: MessageId = MessageId(0x1000_0001);
/// The input action that means cancel/quit; every *other* action advances a frame instead.
pub const ACTION_QUIT: u32 = 0x27;
/// The character the screen's key handler acts on.
pub const CHAR_ESCAPE: u16 = 0x1B;

/// The two input maps the constructor registers with the input manager, undone in the
/// destructor.
///
/// Both at **the focused-UI input priority (3000)**: the constructor registers map 9 and then
/// map 3, each at priority 3000. **That priority is the whole of the click path**: map 3 is the
/// mouse map, the element manager's init registers it at the lowest priority (0), and the
/// qualified-control better-match test
/// returns `false` for two identical bindings — so for a left button
/// (`DIMOFS_BUTTON0`, action **7**) the walk keeps the *first* entry it met, which is this screen's.
/// The action handler then advances one state, because 7 is not [`ACTION_QUIT`].
///
/// Map 9 is the pre-game map and carries exactly two bindings in the shipped `DefaultMap`:
/// `DIK_ESCAPE` → action `0x27` (skip outright) and `DIK_RETURN` → action `0x25`
/// (advance one).
pub const INPUT_MAPS: [u32; 2] = [9, 3];
/// The priority both of [`INPUT_MAPS`] is registered at — the focused-UI input priority.
pub const INPUT_MAP_PRIORITY: i32 = 3000;

/// The intro field's property `0x10000047` — the authored intro state list.
pub const ATTR_STATE_LIST: u32 = 0x1000_0047;
/// The property name every member of that array carries.
pub const ATTR_STATE_LIST_MEMBER: u32 = 0x1000_0048;

/// The four state ids the shipped `intro` layout authors, in list order.
///
/// Not used to *drive* anything — [`IntroScreen::create`] reads the live element — but named here so
/// a test can assert that the dat still says what this module's documentation says it does.
pub const SHIPPED_STATES: [u32; 4] = [0x1000_0038, 0x1000_0039, 0x1000_003A, 0x1000_003E];

/// `IntroScreen` — mode `0x10000001`.
#[derive(Debug, Default)]
pub struct IntroScreen {
    roots: Vec<ElemHandle>,
    /// The state list, read from the root's attribute [`ATTR_STATE_LIST`] in [`Self::create`].
    pub states: std::collections::VecDeque<u32>,
    /// The media state `next_frame` last drove the root element to.
    pub current_state: Option<u32>,
    /// Set once `next_frame` ran out of states.
    pub finished: bool,
    /// Whether the constructor found a state list at all. False means the intro is one frame long,
    /// which is the client's own "empty list ⇒ queue mode `0x1000000A`" branch.
    pub had_state_list: bool,
}

impl IntroScreen {
    /// The factory registered for this screen class.
    #[must_use]
    pub fn create_screen() -> Box<dyn Screen> {
        Box::new(Self::default())
    }

    /// Seed the media-state list by hand. Production reads it off the element; a test that has no
    /// dat uses this.
    pub fn set_states(&mut self, states: &[u32]) {
        self.states = states.iter().copied().collect();
        self.had_state_list = !states.is_empty();
    }

    /// The root element, once `create` has run.
    #[must_use]
    pub fn root(&self) -> Option<ElemHandle> {
        self.roots.first().copied()
    }

    /// The intro field's property `0x10000047` and the enumeration of its members.
    ///
    /// A member whose value is not an `Enum` is skipped rather than guessed at; the shipped layout
    /// has none.
    #[must_use]
    pub fn read_state_list(ui: &UiSystem, root: ElemHandle) -> Vec<u32> {
        use dereth_assets::ui::PropertyValue;
        let Some(node) = ui.node(root) else {
            return Vec::new();
        };
        let props = node.merged_properties();
        let Some(PropertyValue::Array(members)) = props.get(ATTR_STATE_LIST) else {
            return Vec::new();
        };
        members
            .iter()
            .filter_map(|m| match m.value {
                PropertyValue::Enum(v) => Some(v),
                _ => None,
            })
            .collect()
    }

    /// Advance one frame — "pops the head of the list and drives the root element's
    /// media machine to the next state. When the list is empty
    /// `next_frame` queues mode `0x1000000A`."
    ///
    /// The root's four state records all carry the pass-to-children flag, which is how one call
    /// on the 800×600 field starts the movie on the 640×480 child inside it.
    ///
    /// **The state's own `0x3B` is what shows each frame's child**. In `classic_intro`
    /// (`0x21000001`) both media children — `0x10000444` (the movie field) and `0x10000434` (the
    /// splash field) — carry `0x3B = true` at *element* level and `0x3B = false` in every state
    /// that owns media. `0x3B` is `UICore_Element_hide`, so that reads "hidden, except in my own
    /// frame", and the state change above is the whole mechanism. Do not restore the element-level
    /// value after the state change: `0x3B` means *hidden*, and reading it the other way up
    /// produces an intro of fifteen seconds of black.
    pub fn next_frame(&mut self, ui: &mut UiSystem) -> Option<UiMode> {
        match self.states.pop_front() {
            Some(s) => {
                self.current_state = Some(s);
                if let Some(root) = self.roots.first().copied() {
                    ui.set_state(root, StateId(s));
                }
                None
            }
            None => {
                self.finished = true;
                Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT)
            }
        }
    }

    /// The action handler — "any input action ≠ `0x27`" advances a frame; `0x27`
    /// queues character management directly. A release (not a press) is declined.
    ///
    /// **This is the click.** A left button is action **7** of input map 3, a right button 8, a
    /// middle 9 and the double-clicks 10/11/12;
    /// none of them is `0x27`, so every one of them **advances one state** rather than
    /// skipping to the end. The press edge only: a release is declined, so a click is one
    /// advance and not two. Without this call the intro ignores the mouse entirely.
    pub fn on_action(&mut self, ui: &mut UiSystem, action: u32) -> Option<UiMode> {
        if action == ACTION_QUIT {
            Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT)
        } else {
            self.next_frame(ui)
        }
    }

    /// The character handler — Esc queues the mode **and then** calls
    /// `next_frame`, in that order: Esc queues mode `0x1000000A`, then `next_frame` runs
    /// unconditionally.
    ///
    /// **`next_frame` runs whether or not the key was Esc**, so *any* character advances one state and Esc also
    /// queues the skip. The constructor is what makes characters arrive at all:
    /// it turns text mode on and registers its character handler with flag `0x02`, which is
    /// the **character** list, not the action list, so this handler is the one map-independent
    /// route into the screen. The shell otherwise turns text mode on only for a focused editable
    /// `TextElement`, of which the intro has none, so without it no keystroke reaches the intro.
    pub fn character(&mut self, ui: &mut UiSystem, ch: u16) -> Option<UiMode> {
        if ch != CHAR_ESCAPE {
            // The client still calls `next_frame` here; the caller does that with the `None`.
            self.next_frame(ui);
            return None;
        }
        let queued = dereth_ui::framework::mode::CHARACTER_MANAGEMENT;
        self.next_frame(ui);
        Some(queued)
    }
}

impl Screen for IntroScreen {
    /// The intro's action handler. The release edge is declined here exactly as the handler's
    /// first line declines it, so it falls through to the manager and one press-and-release is
    /// **one** advance.
    fn on_mode_action(
        &mut self,
        cx: &mut ScreenCx<'_>,
        e: &dereth_input::InputEvent,
    ) -> Option<dereth_ui::framework::ModeAction> {
        if !e.start {
            return Some(dereth_ui::framework::ModeAction {
                consumed: false,
                handled: false,
                queue: None,
            });
        }
        let queue = self.on_action(cx.ui, e.action.0);
        Some(dereth_ui::framework::ModeAction {
            consumed: true,
            handled: true,
            queue,
        })
    }

    /// The intro's character handler, registered on the character list.
    fn on_mode_character(&mut self, cx: &mut ScreenCx<'_>, ch: u16) -> Option<Option<UiMode>> {
        Some(self.character(cx.ui, ch))
    }

    fn create(&mut self, cx: &mut ScreenCx<'_>) -> Result<(), UiError> {
        let ui = &mut *cx.ui;
        let root = ui
            .require_env()
            .and_then(|e| e.create_and_add_root_element(ui, LAYOUT, ROOT))?;
        self.roots.push(root);
        // Creating the root by DataID registers the framework as
        // a listener on the root, which is what makes the media machine's own `0x1000000D` reach
        // the screen's element-message handler; without it no screen hears its own media.
        ui.register_for_element_messages(root, ME);

        // The constructor's own body: read the state list, then either queue character management
        // (empty) or show state[0].
        let states = Self::read_state_list(ui, root);
        self.had_state_list = !states.is_empty();
        self.states = states.into_iter().collect();
        if self.states.is_empty() {
            // An empty list queues mode `0x1000000A` — reported through the request
            // queue because a `Screen` has no `queue_ui_mode`.
            self.finished = true;
            ui.requests.emit(crate::view::UiRequest::QueueMode(
                dereth_ui::framework::mode::CHARACTER_MANAGEMENT,
            ));
        } else {
            // Otherwise pop the head state id and put the intro field in that state.
            self.next_frame(ui);
        }
        Ok(())
    }

    fn destroy(&mut self, cx: &mut ScreenCx<'_>) {
        let ui = &mut *cx.ui;
        for r in std::mem::take(&mut self.roots) {
            ui.unregister_from_element(r, ME);
        }
    }

    fn on_element_message(&mut self, cx: &mut ScreenCx<'_>, m: &ElementMessage) {
        let ui = &mut *cx.ui;
        let queued = if m.id == MSG_SKIP {
            Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT)
        } else if m.id == MSG_MEDIA_FINISHED {
            self.next_frame(ui)
        } else {
            None
        };
        if let Some(mode) = queued {
            ui.requests.emit(crate::view::UiRequest::QueueMode(mode));
        }
    }

    fn roots(&self) -> &[ElemHandle] {
        &self.roots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered screen catalogue's trigger table and the layout-enum map (enum
    /// `0x10000002` → `0x21000001` `intro`, whose only root element is `0x10000419`).
    #[test]
    fn the_screen_names_the_documented_layout_root_and_triggers() {
        assert_eq!(LAYOUT, LayoutEnum(0x1000_0002));
        assert_eq!(ROOT, ElementId(0x1000_0419));
        assert_eq!(MSG_MEDIA_FINISHED, MessageId(0x1000_000D));
        assert_eq!(MSG_SKIP, MessageId(0x1000_0001));
        assert_eq!(ACTION_QUIT, 0x27);
        assert_eq!(CHAR_ESCAPE, 0x1B);
        assert_eq!(INPUT_MAPS, [9, 3]);
        assert_eq!(ATTR_STATE_LIST, 0x1000_0047);
    }

    /// Oracle: the frame advance — one state per advance, and the empty list queues character
    /// management.
    #[test]
    fn the_media_list_advances_one_state_per_frame_advance_then_queues_char_management() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = IntroScreen::default();
        s.set_states(&[10, 20, 30]);
        assert_eq!(s.next_frame(&mut ui), None);
        assert_eq!(s.current_state, Some(10));
        assert_eq!(s.next_frame(&mut ui), None);
        assert_eq!(s.current_state, Some(20));
        assert_eq!(s.next_frame(&mut ui), None);
        assert_eq!(s.current_state, Some(30));
        assert_eq!(
            s.next_frame(&mut ui),
            Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT)
        );
        assert!(s.finished);
    }

    /// Oracle: §4's trigger table — `OnAction` splits on `0x27`, and Esc queues *then* advances.
    #[test]
    fn skipping_queues_character_management_by_every_documented_route() {
        let mut ui = UiSystem::new((800, 600));
        let cm = dereth_ui::framework::mode::CHARACTER_MANAGEMENT;

        let mut s = IntroScreen::default();
        s.set_states(&[1, 2]);
        assert_eq!(
            s.on_action(&mut ui, 5),
            None,
            "any other action just advances"
        );
        assert_eq!(s.current_state, Some(1));
        assert_eq!(s.on_action(&mut ui, ACTION_QUIT), Some(cm));

        let mut s = IntroScreen::default();
        s.set_states(&[1, 2]);
        assert_eq!(s.character(&mut ui, CHAR_ESCAPE), Some(cm));
        assert_eq!(s.current_state, Some(1), "Esc also advances a frame");
        assert_eq!(s.character(&mut ui, b'a'.into()), None);
    }

    /// Oracle: §4's trigger table — element message `0x10000001` skips, `0x1000000D` advances.
    #[test]
    fn the_two_element_messages_do_what_the_table_says() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = IntroScreen::default();
        s.set_states(&[1]);
        let msg = |id: MessageId| ElementMessage {
            source_id: ROOT,
            source: ElemHandle::for_test(1),
            id,
            p1: 0,
            p2: 0,
            point: dereth_ui::msg::MessagePoint::default(),
            serial: 1,
        };
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(MSG_MEDIA_FINISHED),
        );
        assert!(
            ui.requests.take().is_empty(),
            "one state left, so it just advances"
        );
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(MSG_MEDIA_FINISHED),
        );
        assert_eq!(
            ui.requests.take(),
            vec![crate::view::UiRequest::QueueMode(
                dereth_ui::framework::mode::CHARACTER_MANAGEMENT
            )]
        );
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(MSG_SKIP),
        );
        assert_eq!(
            ui.requests.take(),
            vec![crate::view::UiRequest::QueueMode(
                dereth_ui::framework::mode::CHARACTER_MANAGEMENT
            )]
        );
    }

    /// Oracle: the shipped `intro` layout itself, synthesised here as the same `Array` of
    /// `0x10000048` `Enum`s that `client_local_English.dat` object `0x21000001` carries on element
    /// `0x10000419`. `dereth/client/tests/gpu/login/intro_sequence.rs` reads the real dat; this proves the *reader*
    /// against the documented shape with no dat present.
    #[test]
    fn the_state_list_is_read_out_of_attribute_0x10000047() {
        use dereth_assets::ui::{BaseProperty, PropertyValue};
        let mut ui = UiSystem::new((800, 600));
        let h = ui.create_hollow(None);
        let array = PropertyValue::Array(
            SHIPPED_STATES
                .iter()
                .map(|s| BaseProperty {
                    id: ATTR_STATE_LIST_MEMBER,
                    value: PropertyValue::Enum(*s),
                })
                .collect(),
        );
        if let Some(n) = ui.node_mut(h) {
            n.instance_properties.set(ATTR_STATE_LIST, array);
        }
        assert_eq!(
            IntroScreen::read_state_list(&ui, h),
            SHIPPED_STATES.to_vec()
        );
        // An element with no such attribute yields the empty list, which is the client's
        // "no intro, go straight to character select" branch.
        let bare = ui.create_hollow(None);
        assert!(IntroScreen::read_state_list(&ui, bare).is_empty());
    }
}
