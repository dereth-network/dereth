//! The UI shell: the element manager's slot in the frame and the UI-flow mode machine.
//!
//! The UI preserves the eight modes, deferred switching, destroy-then-create order, and every edge
//! of the state diagram. Main-loop step 7 broadcasts global message 3, where the mode state
//! machine advances.
//!
//! **This module wires; it does not implement.** Every decision here belongs to `dereth-ui` or
//! `dereth-ui-screens`:
//! `UiFlow` is `dereth_ui::framework::UiFlow`, the eight screens and the 84 game element types are
//! registered by `dereth_ui_screens::register_all`, the layout-enum indirection is
//! `DidMapperResolver::load_via_master`, and the draw list is `UiSystem::draw`. What this module
//! owns is the three things the client reads out of **process globals** and a rebuild has to hand
//! over explicitly:
//!
//! 1. the data-patch screen asks the network layer whether the socket is connected and checks
//!    whether UI-flow persistent data has received `0xF658`;
//! 2. the persistent-data character-set notice is fed by the network, not by the UI;
//! 3. the server-died and character-error notices are
//!    the two "any → Disconnected" edges, and both come from the session.
//!
//! [`HostState`] is those three, and [`UiShell::frame`] is the only place they are applied.
//!
//! # What is deliberately not here
//!
//! The connect and patch screens' real states, character select and creation, the HUD and the
//! panels, and picking. This module is the seam those plug into: it brings the crates into the
//! binary, gives the mode machine a place in the documented frame, and proves the four paths end
//! to end.

use std::rc::Rc;
use std::sync::Arc;

use dereth_primitives::LocalTime;
use dereth_ui::framework::{mode, DidMapperResolver, UiMode};
use dereth_ui::{RecordingDrawBackend, UiDrawCmd, UiFlow, UiSystem};
use dereth_ui_screens::screens::chargen::{CharGenAction, CharGenTables};
use dereth_ui_screens::screens::charmgmt::CharacterAction;
use dereth_ui_screens::view::UiRequest;

// ---------------------------------------------------------------------------------------------
// The mouse reaches the tree
// ---------------------------------------------------------------------------------------------

/// What the UI element manager reads from input state once per frame, and the two handler slots it
/// occupies there.
///
/// UI initialization does two things this trait stands for:
/// an input handler registered with flags `0x11` — flag `0x01` **action** and flag `0x10` **mouse
/// move** — and
/// input map 3 at `priority::LOWEST` (0), with the element manager as callback. Everything the UI
/// gets from the mouse arrives through one of those two registrations:
///
/// | what | client route | method |
/// |---|---|---|
/// | the pointer moved | step 3 of the input poll → the mouse-move handler | [`Self::take_mouse_move`] |
/// | the pointer left the window | the window procedure's `WM_MOUSELEAVE` row | [`Self::take_mouse_left_window`] |
/// | a button went down or up | input map 3 → the UI callback | [`Self::take_actions`] |
/// | where the pointer is | the input manager's pointer-position query | [`Self::mouse_pos`] |
///
/// It exists because [`dereth_ui::InputPump`] carries only `use_time` — `dereth-ui` deliberately
/// cannot see `dereth-input` — and because [`UiShell::frame`] must dispatch the actions **inside** the frame,
/// between the input update and the queued deliveries, or every click a screen
/// answers is a frame late.
pub trait UiInput: dereth_ui::InputPump {
    /// The same object as an [`dereth_ui::InputPump`], for the input update.
    ///
    /// A supertrait upcast written out: `UiSystem::use_time` takes the narrow trait and this one
    /// widens it, which keeps `dereth-ui`'s seam narrow.
    fn as_pump(&mut self) -> &mut dyn dereth_ui::InputPump;
    /// The stored mouse position, which every hit test uses.
    fn mouse_pos(&self) -> (i32, i32);
    /// The mouse-move handler's `(x, y)` in absolute client coordinates, or `None` when the pointer
    /// did not move this frame.
    fn take_mouse_move(&mut self) -> Option<(i32, i32)>;
    /// A `WM_MOUSELEAVE` arrived.
    fn take_mouse_left_window(&mut self) -> bool;
    /// The actions the input-event dispatcher produced, oldest first.
    fn take_actions(&mut self) -> Vec<dereth_input::InputEvent>;
    /// Hand back the ones returned **false** for.
    ///
    /// Retail offers each action to the callback first, and only when there is no callback or it
    /// declines does the action go on to the handler list.
    /// The UI's callback consumes what it acts on and declines the rest, and only the rest is any
    /// other subsystem's business.
    fn put_back_unconsumed(&mut self, events: Vec<dereth_input::InputEvent>);
    /// The accepted characters for delivery to the focused text element.
    fn take_characters(&mut self) -> Vec<char> {
        Vec::new()
    }
    /// Set text mode, the gate character updating tests before delivery.
    fn set_text_mode(&mut self, _on: bool) {}
    /// The focus manager's gain/lose pair: the maps the element
    /// that now holds focus registers through its focus-gain override,
    /// and nothing else. An empty slice is the loser's focus-loss override.
    ///
    /// It takes the **set** rather than a `bool` because which maps a focused element pushes is a
    /// property of the *element*, not of text mode:
    /// every scrollable pushes `0x0A`, and only editable or selectable text adds maps 1, 7, and 8
    /// on top, behind two different element-flag tests. See
    /// [`UiShell::focused_input_maps`].
    fn set_focused_input_maps(&mut self, _maps: &[(u32, i32)]) {}
    /// The maps activation registers at priority 0 and
    /// the activation alert registers at 2000 on the newly **active** element,
    /// and deactivation's counterpart when the slice is empty.
    ///
    /// `(map, priority)` pairs rather than bare maps, because this band is not one priority: the
    /// element's own input map goes in at the value it was handed and each ancestor's one
    /// lower. See [`UiShell::active_input_maps`].
    fn set_active_input_maps(&mut self, _maps: &[(u32, i32)]) {}
    /// The maps the current pre-game screen's *constructor* registers with the input manager, and
    /// its destructor takes away. An empty slice is the destructor.
    ///
    /// Bare maps rather than `(map, priority)` pairs because all four registrations are the same
    /// priority 3000, `priority::FOCUSED_UI`; the registration behavior is described by
    /// [`crate::input::InputShell::set_mode_input_maps`] and the set is
    /// [`PREGAME_MODE_INPUT_MAPS`].
    fn set_mode_input_maps(&mut self, _maps: &[u32]) {}
    /// Begin (`true`) or end the character session's input maps — movement, item selection,
    /// the panel toggles, the quickbar, emotes, combat and chat. See
    /// [`crate::input::InputShell::set_character_session_input_maps`]; the UI mirrors "the gameplay
    /// screen is up" into it.
    fn set_character_session_input_maps(&mut self, _live: bool) {}
    /// Enable or disable target mode's priority-2000 mouse callback.
    fn set_target_input_map(&mut self, _active: bool) {}
    /// The action-dispatch wrapper's head, which is the only thing that can arm the
    /// ignore-next-character latch.
    ///
    /// The client's wrapper copies the key-down-in-progress flag into an action-dispatch latch
    /// around the action-callback and handler-list walk. Text-mode switching reads that latch when
    /// a handler turns text mode on. Retail can read the flag live because the walk runs
    /// inside the `WM_KEYDOWN`; this shell dispatches from the frame, so the key-down context is
    /// carried on the input event's key-down-origin flag and restored here.
    fn begin_action_dispatch(&mut self, _from_key_down: bool) {}
    /// The matching tail. See [`Self::begin_action_dispatch`].
    fn end_action_dispatch(&mut self) {}
}

/// The UI element manager's own input map — map 3 at `priority::LOWEST` during initialization.
///
/// Gating [`UiShell::route_input`] on it is what makes this dispatch
/// rather than "any action that happens to look like a click": in the client the UI's action handler is reached
/// **only** as the callback of the map that won, and map 3 is the UI's. `0x1000000B`
/// (target mode) also binds `DIMOFS_BUTTON0` to action 7 and belongs to UI input. Its
/// callback observes the leave edge before forwarding the mouse action to the UI manager.
pub const UI_INPUT_MAP: dereth_input::InputMapId = dereth_input::InputMapId(3);

/// Host calls that retail makes synchronously through global UI and item-holder state.
/// No element callback is active when these run: external listeners are delivered after the
/// UiSystem broadcast returns, before the input manager sends its next action to listeners.
#[derive(Debug, Clone, Copy)]
pub enum UiDispatch {
    /// Complete emitted requests and refresh input facts before the next screen delivery.
    Requests,
    /// Target mode observes the button before the manager delegates it.
    Mouse(UiMouseEvent),
    /// Refresh the world-hover request after the base hover and hit state was updated.
    Hover((i32, i32)),
}

type DispatchHook<'a> = dyn FnMut(&mut UiShell, UiDispatch) + 'a;

/// The maps the original intro screen registers **with itself** as the callback, at
/// `priority::FOCUSED_UI` (3000) — above the element manager's map 3 at `priority::LOWEST`.
///
/// The map stack inserts descending by priority and the binding comparison returns
/// `false` when two candidate bindings are
/// identical, so for a control both entries bind — the left mouse button, `DIMOFS_BUTTON0` in
/// map 3 — the walk keeps the **first** it met, which is the screen's. That is the whole
/// mechanism by which a click on the intro reaches the intro screen before the UI manager, and it
/// is why this gate is a *map* test and not a "is the
/// intro up, swallow everything" test: only these two maps are the screen's, and only while it is.
///
/// A full mirror of the client's registration list would put this in
/// [`crate::input::BASE_MAP_REGISTRATIONS`] and read the winning callback off the event; the event
/// carries no callback (`dereth_input::InputEvent`), so the ownership is expressed here instead. The
/// observable is the same because these two maps have exactly one other registrant between them.
pub const INTRO_INPUT_MAPS: [u32; 2] = dereth_ui_screens::screens::intro::INPUT_MAPS;

/// **The input maps each pre-game screen's constructor registers with the input manager, and its
/// destructor takes away.** This is the producer [`INTRO_INPUT_MAPS`]' map-9 arm filters for.
///
/// The client has exactly **four** screen-lifetime map registrations at priority `0xBB8` outside
/// the element manager and the text/scrollable pair, and all four are in these three constructors —
/// they are listed on [`crate::input::InputShell::set_mode_input_maps`]. Nothing else in the
/// client registers input map **9** with a literal.
///
/// Read the table as the client's own registration order within each screen: the intro pushes
/// **9 before 3**, and
/// [`dereth_input::dispatch::InputMapStack::register`] prepends an equal-priority newcomer, so the
/// walk sees **3 before 9** — which is what it must be, because map 3's `DIMOFS_BUTTON0` and
/// map 9's `DIK_RETURN` are different devices and the order between them is never load-bearing,
/// while map 3 at 3000 must stay in front of the element manager's copy at `priority::LOWEST` for
/// `UiShell::mode_on_action` to see a click as the intro's.
///
/// **What each screen does with the two controls map 9 carries**, read out of the three action
/// handlers rather than assumed — and it is *not* uniform, which is why every assertion about this
/// row is per screen:
///
/// | screen | `EscapeKey` `0x27` | `AcceptInput` `0x25` |
/// |---|---|---|
/// | intro | queue mode `0x1000000A` — skip the intro | not `0x27`, so advance one media state |
/// | credits | reads the event **nowhere**: unregister global 3, show the please-wait dialog, queue mode `0x1000000A` | the same, because the body never looks at the action |
/// | character management | show the confirm-exit dialog, return **true** | return `action == 0x27`, i.e. **false** — declined, and the handler list receives it |
///
/// So *"Enter answers the dialog"* is true of two of the three and false of the third, and the
/// third is the client's own behaviour: the original dialog class has no action-handler override
/// (its implementation has construction, destruction, element/global-message handlers, layout
/// updates, and data assignment, but no action-handler override), and no dialog subclass compares an input
/// action against `0x25` anywhere in the original. A dialog is answered by **element message
/// 1 from a button child**, which is `DialogKind::answer_children`.
pub const PREGAME_MODE_INPUT_MAPS: &[(UiMode, &[u32])] = &[
    (mode::INTRO, &INTRO_INPUT_MAPS),
    (
        mode::CREDITS,
        &[dereth_ui_screens::screens::credits::INPUT_MAP],
    ),
    (
        mode::CHARACTER_MANAGEMENT,
        &[
            dereth_ui_screens::screens::charmgmt::INPUT_MAP,
            CHARACTER_SCREEN_SCROLL_MAP,
        ],
    ),
];

/// The scrollable controls, which the character screen registers beside its own map so that the
/// mouse wheel scrolls what is under the pointer (the world's message, the character list) with
/// nothing focused. The retail screen registered map 9 alone; a scrollable registered these only
/// while it had the focus. Its events are not the screen's: they take the element manager's road,
/// as a focused scrollable's wheel does.
pub const CHARACTER_SCREEN_SCROLL_MAP: u32 = crate::input::SCROLLABLE_INPUT_MAP.0;

impl UiInput for crate::input::InputShell {
    fn as_pump(&mut self) -> &mut dyn dereth_ui::InputPump {
        self
    }
    fn mouse_pos(&self) -> (i32, i32) {
        crate::input::InputShell::mouse_pos(self)
    }
    fn take_mouse_move(&mut self) -> Option<(i32, i32)> {
        match self.take_mouse_frame() {
            dereth_input::mouse::MouseFrameAction::Move { x, y } => Some((x, y)),
            // `Look`/`LookIdle` go to the mouse-look handler and the camera, never to the UI, and
            // `None` is "the pointer did not move".
            _ => None,
        }
    }
    fn take_mouse_left_window(&mut self) -> bool {
        self.take_mouse_left_window()
    }
    fn take_actions(&mut self) -> Vec<dereth_input::InputEvent> {
        self.take_events()
    }
    fn put_back_unconsumed(&mut self, events: Vec<dereth_input::InputEvent>) {
        self.put_back_unconsumed(events);
    }
    fn take_characters(&mut self) -> Vec<char> {
        crate::input::InputShell::take_characters(self)
    }
    fn set_text_mode(&mut self, on: bool) {
        crate::input::InputShell::set_text_mode(self, on);
    }
    fn set_focused_input_maps(&mut self, maps: &[(u32, i32)]) {
        crate::input::InputShell::set_focused_input_maps(self, maps);
    }
    fn set_active_input_maps(&mut self, maps: &[(u32, i32)]) {
        crate::input::InputShell::set_active_input_maps(self, maps);
    }
    fn set_mode_input_maps(&mut self, maps: &[u32]) {
        crate::input::InputShell::set_mode_input_maps(self, maps);
    }
    fn set_character_session_input_maps(&mut self, live: bool) {
        crate::input::InputShell::set_character_session_input_maps(self, live);
    }
    fn set_target_input_map(&mut self, active: bool) {
        crate::input::InputShell::set_target_input_map(self, active);
    }
    fn begin_action_dispatch(&mut self, from_key_down: bool) {
        self.manager.begin_action_dispatch(from_key_down);
    }
    fn end_action_dispatch(&mut self) {
        self.manager.end_action_dispatch();
    }
}

/// The bring-up and test pump: no device, so no mouse and no actions.
impl UiInput for dereth_ui::NullInputPump {
    fn as_pump(&mut self) -> &mut dyn dereth_ui::InputPump {
        self
    }
    fn mouse_pos(&self) -> (i32, i32) {
        (0, 0)
    }
    fn take_mouse_move(&mut self) -> Option<(i32, i32)> {
        None
    }
    fn take_mouse_left_window(&mut self) -> bool {
        false
    }
    fn take_actions(&mut self) -> Vec<dereth_input::InputEvent> {
        Vec::new()
    }
    fn put_back_unconsumed(&mut self, _events: Vec<dereth_input::InputEvent>) {}
}

/// Anything that stops the UI from coming up.
///
/// Failing to initialize the UI is fatal: a client with no UI has no way to reach any screen.
#[derive(Debug, thiserror::Error)]
pub enum UiShellError {
    /// `MasterProperty 0x39000001` — "the layout stream is undecodable without it".
    #[error("the property-type table (MasterProperty 0x39000001) is unavailable: {0}")]
    PropertyTypes(String),
    /// The two-level lookup that resolves a layout enum.
    #[error("the UI layout mapper is unavailable: {0}")]
    Resolver(String),
}

/// What the application knows and the pre-game screens read out of globals — the pre-game view.
///
/// It lives in `dereth_client_contract::pregame` as [`dereth_client_contract::pregame::PregameView`], because the
/// screens that read it are below this crate; this is its historical name and path. The host
/// assembles one per frame and [`UiShell::frame`] hands it to the current screen through
/// [`dereth_ui::framework::Screen::on_pregame`].
pub use dereth_client_contract::pregame::PregameView as HostState;

/// Counters that must be asserted on rather than merely logged.
///
/// The project's standing rule: "if you make a decode or lookup tolerant of failure, give it a
/// counter **and assert on that counter**." Three things here are tolerant by design and every one
/// of them has a number:
///
/// * `UiFlow::use_new_mode` silently drops an unknown mode id —
///   [`UiStats::unregistered_mode_requests`];
/// * `use_new_mode` also swallows a screen whose `create` fails, leaving no current screen —
///   [`UiStats::screen_create_failures`];
/// * a `UiRequest` this shell does not yet act on is not an error but must not vanish —
///   [`UiStats::requests_ignored`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiStats {
    /// How many times the mode switch actually swapped the framework.
    pub mode_switches: u64,
    /// A queued mode that is not in the flow's screen-constructor table. Must stay zero: "No such id is
    /// ever queued in this build."
    pub unregistered_mode_requests: u64,
    /// A screen whose `create` failed, so the flow has a current mode and no current screen. Must
    /// stay zero: it means a layout would not load.
    pub screen_create_failures: u64,
    /// `UiRequest`s acted on.
    pub requests_handled: u64,
    /// `UiRequest`s this shell has no owner for yet. Named in the report rather than dropped.
    pub requests_ignored: u64,
    /// Media sound steps raised by a real element
    /// state change and handed to the sound track.
    ///
    /// Counted because the two ends fail differently and both are silent: a zero here means no
    /// button ever reached its pressed state's media list, and a non-zero here with no voice means
    /// the table or the wave would not come out of the dat.
    pub media_sounds: u64,
    /// Device shutdown was requested by the epilogue screen.
    pub device_done: bool,
    /// Player-session calls made by the character-management screen: log on, delete, restore.
    pub character_actions: u64,
    /// `0xF656` sends and post-creation log-ons the char-gen wizard asked for.
    pub chargen_actions: u64,
    /// The char-gen dat tables would not load, so the wizard can choose nothing. Must stay zero.
    pub chargen_table_failures: u64,
    /// [`dereth_ui::UiSystem::mouse_down`] calls made from a real pointer event.
    ///
    /// Counted rather than logged for the same reason as the rest of this struct: the number is
    /// the difference between "a click reached the tree" and "a click reached the tree and the
    /// hit test found nothing", and only the first is this shell's business to guarantee.
    pub mouse_downs: u64,
    /// Screen-layout loads whose file **opened**.
    ///
    /// Counted rather than logged because "the layout was applied" and "there was no file yet" are
    /// different states and only one of them is a defect: a first run legitimately has no
    /// `UI-<char>-<world>-<h>-<w>.txt`, represented by a false layout-from-file flag.
    pub screen_layout_loads: u64,
    /// Of those, how many of the sixteen windows were actually placed — the denominator without
    /// which "the file loaded" says nothing about whether it did anything.
    pub screen_layout_windows_placed: u64,
    /// Screen-layout saves that wrote a file.
    pub screen_layout_saves: u64,
    /// [`dereth_ui::UiSystem::mouse_up`] calls. See [`UiStats::mouse_downs`].
    pub mouse_ups: u64,
    /// UTF-16 code units handed to the focused text element.
    ///
    /// Two independent things must both work for this to move: text mode must come on, and
    /// [`dereth_ui::UiSystem::character`] must be called.
    pub characters_delivered: u64,
    /// Press-edge calls, i.e. [`dereth_ui::UiSystem::key_press`] calls made from a real input
    /// event.
    ///
    /// `on_action` returns `false` for anything that is not a mouse button, so without the
    /// key-press path [`UiShell::route_input`] would hand every key straight back. The counter is
    /// here so "the keyboard reaches the tree" is a number rather than a feeling.
    pub key_presses: u64,
    /// Of those, the ones the focused/active element declined, so
    /// global message 1 went out with the action. This is the number the eighteen quickbar
    /// hotkeys, `handle_key_press` and the epilogue's any-key all live on.
    pub key_presses_broadcast: u64,
    /// The action handler's release leg — the focused/active element declined, so
    /// global message 2 went out with the action.
    pub key_releases_broadcast: u64,
    /// `IntroScreen::on_action` calls made from a real input event — the click that advances the
    /// intro.
    pub intro_actions: u64,
    /// Intro character-handler calls. Zero whenever text mode is off while the intro is up,
    /// because the character update then delivers nothing.
    pub intro_characters: u64,
    /// Credits action calls made from a real input event, through [`UiShell::mode_on_action`]
    /// rather than through a global-message-1 stand-in.
    ///
    /// A denominator with a specific job: the credits' `finished` flag makes a second delivery
    /// invisible in `mode_switches`, which is a single slot, so *"answered once"* and *"answered
    /// twice and the second was idempotent"* read alike without this.
    pub credits_actions: u64,
    /// Character-management action calls, the same way, counting the ones it **declines** as
    /// well as the ones it takes — the declines are the evidence that Enter still falls through to
    /// the input-handler list, which is what
    /// `return action == 0x27` does in retail.
    pub charmgmt_actions: u64,
    /// Actions a container took away from the focused element through its child action handler.
    /// Without the bubble that reaches a child-action handler, every one of them is dead code.
    pub child_actions_consumed: u64,
    /// Text-mode calls this shell made, counting **edges** and not frames.
    ///
    /// A denominator for [`UiShell::sync_text_mode`]: "text mode never came on" and "text mode came
    /// on and the character was dropped anyway" are the same observation without it.
    pub text_mode_edges: u64,
    /// Of those, the ones the **second** mirror made — the one that runs after
    /// `flow.deliver` / `flow.update` / the mode switch, where retail's focus take actually
    /// happens. Every one of these is a character that would otherwise have been dropped.
    pub text_mode_edges_late: u64,
    /// The focus setter's register/unregister pair, counted as
    /// **edges** of the focused map *set*.
    ///
    /// Its own counter rather than a share of [`Self::text_mode_edges`], because the two are
    /// different questions: focusing the chat **log** moves this and not that. A denominator, so "the wheel did nothing" and "no registration was
    /// ever attempted" cannot read alike.
    pub focused_map_edges: u64,
    /// The same, for the `priority::UNFOCUSED_UI` band — activation's
    /// priority-0 maps / the activation alert's priority-2000 maps and
    /// deactivation's counterpart, counted as edges of the active map *set*.
    ///
    /// A denominator with a specific job: with the shipped layouts the set is empty on both sides
    /// of every activation (attribute `0x4E` is on 2 of 2 162 elements and on no root element), so
    /// this counter reads **0** in a running client and that is correct rather than a missing wire.
    /// Without it, "the band is empty because nothing carries a map" and "the mirror was never
    /// called" are the same observation. See [`UiShell::active_input_maps`].
    pub active_map_edges: u64,
    /// The same, for the pre-game screens' own band — the
    /// map 9 at priority 3000 that each of the three constructors registers and its destructor
    /// undoes, counted as edges of [`PREGAME_MODE_INPUT_MAPS`]' set.
    ///
    /// A denominator with a job the other two do not have: this set is **non-empty** on three of
    /// the eight registered modes and empty on the rest, so a stuck `0` means the mirror never ran
    /// while a stuck non-zero would mean it is running every frame instead of on the edge. It is
    /// what separates "Enter reached no `AcceptInput` because map 9 is not registered" from "…
    /// because the walk found something better".
    pub mode_map_edges: u64,
}

// ---------------------------------------------------------------------------------------------
// The intro movie
// ---------------------------------------------------------------------------------------------

/// The `DataID` the current movie frame is uploaded under.
///
/// **Not a dat id.** The movie media step's no-database-file payload is a **plain file path**
/// resolved relative to the working directory, rather than a DataID,
/// — and the decoded frames go into a UI surface that has no `DataID` at all. `UiDrawCmd::image`
/// is a `DataID`, so the movie needs a handle in that space; `0xFF000000` is a `DivineType` range
/// no dat file uses, which makes a collision
/// with a real object impossible.
pub const MOVIE_IMAGE_ID: dereth_primitives::DataId = dereth_primitives::DataId(0xFF00_0001);

/// What the movie step did, asserted on rather than logged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MovieStats {
    /// Movie media steps reached.
    pub started: u64,
    /// Steps whose file was missing or would not demux. **This is not an error**: "a missing or
    /// unplayable AVI silently skips that intro state, and
    /// `AC-ThroneOfDestiny.avi` is not in this install, so exactly one of the intro's two movie
    /// steps is expected to land here.
    pub skipped: u64,
    /// Frames decoded and uploaded.
    pub frames: u64,
    /// Movies that reached their end and reported that they finished playing.
    pub completed: u64,
}

/// One playing movie media step, represented in the client by its DirectShow movie graph.
///
/// This preserves the movie behaviors the surrounding UI depends on: frame timing is
/// `min(movie fps, client fps)` with no interpolation
/// ([`dereth_audio::video::Movie::next_frame`] decodes forward to whatever frame the wall clock is
/// on), the vertical flip and the forced-opaque alpha are the decoder's, a missing file skips the
/// step silently, and **there is no loop** — always passes flags = 0.
/// What the host should tell the sound device about the movie's soundtrack.
///
/// The retail client has no such message: DirectShow's filter graph carries the audio to the
/// renderer by itself, and movie playback never enters sound management. This is the price of the
/// rebuild keeping the demuxer in `dereth-audio` and the device in the application, and it is
/// deliberately the only way the track starts or stops.
#[derive(Debug)]
pub enum MovieAudioCue {
    /// `IMediaControl::Run()` — the decoded track, already at `MIX_RATE`.
    Start(Box<dereth_audio::video::MovieAudio>),
    /// `IMediaControl::Stop()` — the graph is gone, and so is whatever was left of the track.
    Stop,
}

#[derive(Debug)]
struct MoviePlayer {
    /// The UI element whose surface receives the decoded movie frame.
    element: dereth_ui::ElemHandle,
    movie: dereth_audio::video::Movie,
    /// `IMediaControl::Run()` — when the graph started.
    started: LocalTime,
    /// `avih.dwTotalFrames * avih.dwMicroSecPerFrame`, i.e. when `EC_COMPLETE` is due.
    duration: f64,
}

/// One pointer event dispatched, with the element the hit test found under it.
///
/// The world-view wrapper's own mouse-down/mouse-up arms are the 3D world's half of the same
/// events, and that element type is not
/// implemented in this build, so the events are recorded here and [`crate::interaction`] runs the
/// wrapper's table. Nothing else about the dispatch changes: the tree still sees every event first,
/// and `over` is the last element under the pointer, exactly as the hit test reads it. The type
/// lives beside its consumer, the interaction layer, in `dereth_client_runtime`.
pub use dereth_client_runtime::interaction::UiMouseEvent;

/// The UI element manager, [`UiFlow`], and the host state they read.
///
/// One per process, matching the single UI manager and UI flow.
#[derive(Debug)]
pub struct UiShell {
    /// The UI element manager.
    pub ui: UiSystem,
    /// The files the screens are built from and draw their own art from
    /// ([`dereth_dat::RetailDatStore::interface_files`]).
    pub interface: Arc<dereth_dat::RetailDatStore>,
    /// The eight-mode UI flow.
    pub flow: UiFlow,
    /// Every mode actually entered, in order. This is the recording the acceptance test compares
    /// against the eight-mode state diagram.
    transitions: Vec<UiMode>,
    /// The host state as of the last frame, so a change is applied once rather than every frame.
    last_host: HostState,
    /// Table enum `0x10000002` resolved through group 4 of the master `DidMapper`, which is where
    /// character management uses to build its `StringInfo`. `None` when the mapper has
    /// no such entry, in which case the patch line stays at its layout default.
    patch_strings: Option<dereth_primitives::DataId>,
    /// Table enum `0x10000001`, through the same group.
    ///
    /// A *second* table, because the two logout confirmations are the only strings in this build
    /// that do not live in `0x10000002`: the end-character-session and
    /// logoff handlers both take their string from table `0x10000001`.
    client_strings: Option<dereth_primitives::DataId>,
    /// What the character-management screen asked player-session state for this frame.
    character_actions: Vec<CharacterAction>,
    /// What the char-gen wizard asked for this frame.
    chargen_actions: Vec<CharGenAction>,
    /// Character-generation data and the `SkillTable`, loaded once. `None` when either would not decode.
    chargen_tables: Option<Rc<CharGenTables>>,
    /// The Classic presentation of the same world rules, retained across interface rebuilds.
    classic_creation: Result<Rc<dereth_classic_ui::panels::pregame::data::CreationData>, String>,
    /// The last character set pushed into a screen, so a rebuild happens on the edge.
    last_char_set: Option<dereth_ui::persist::CharacterSet>,
    /// Where the movie media step's no-database-file paths resolve from — the client's working
    /// directory in the original, the retail install directory here.
    pub client_dir: std::path::PathBuf,
    /// Install resources are supplied by the host, independently of the preferences store.
    pub movie_bytes: fn(&std::path::Path) -> Option<Vec<u8>>,
    /// The one movie player this build can have at a time. The client can have one per element;
    /// the shipped layouts play movies only from `IntroScreen`, one state at a time.
    movie: Option<MoviePlayer>,
    /// The frame the host must upload before it draws, taken once per frame.
    movie_frame: Option<dereth_primitives::TextureData>,
    /// What the soundtrack should do, for the host to hand to sound management. See
    /// [`MovieAudioCue`].
    movie_audio_cue: Option<MovieAudioCue>,
    /// Media-machine sound requests waiting for the host.
    ///
    /// Every entry is a [`UiRequest::PlaySound`], in the order the media steps ran. It sits beside
    /// [`Self::movie_audio_cue`] and for the same reason: this crate owns the `MediaPlayback` and
    /// not the sound device, and `dereth_ui`'s [`dereth_ui::MediaEffect::PlaySound`] is raised by the
    /// track that owns neither. Drained once per frame by [`Self::take_sound_requests`], in the
    /// same `App::frame` step the movie cue is taken — so a click's sound starts on the frame the
    /// click happened, which is what retail's synchronous sound update does.
    sound_requests: Vec<UiRequest>,
    pub movie_stats: MovieStats,
    /// Character logoff was asked for by the epilogue screen's constructor.
    log_off_requested: bool,
    /// A keymap save was requested by the Key Bindings page's *OK*
    /// button. The writer is the input manager's keymap save, which is the host's `InputShell`, so
    /// this is a flag and not a call — the same shape [`Self::log_off_requested`] is.
    save_keymap_requested: bool,
    pub stats: UiStats,
    /// The pointer events this frame dispatched, drained by the host.
    mouse_events: Vec<UiMouseEvent>,
    target_mode_active: bool,
    /// Text mode as this shell last set it, so
    /// text mode is set on the **edge** and its ignore-next-character latch is not
    /// re-armed every frame.
    text_mode: bool,
    /// The maps the focused element currently has registered — the focus setter's
    /// half of the state, mirrored so [`UiShell::sync_text_mode`] can write on the **edge**.
    /// `(map, priority)` pairs, because the base-element registration chain gives ancestors'
    /// maps lower priorities.
    focused_maps: Vec<(u32, i32)>,
    /// The `(map, priority)` pairs the **active root element** currently has registered —
    /// the maps activation registers at priority 0 plus the ones the activation alert registers
    /// at 2000, mirrored on the edge for the same reason as
    /// [`Self::focused_maps`].
    active_maps: Vec<(u32, i32)>,
    /// The maps the **current pre-game screen** has registered — the constructor half of
    /// [`PREGAME_MODE_INPUT_MAPS`], mirrored on the mode edge for the same reason as
    /// [`Self::focused_maps`].
    mode_maps: Vec<u32>,
    /// Whether the character session's input maps were last mirrored up (the gameplay screen is
    /// the current mode) or down. `None` until the first mirror, so the first frame always writes:
    /// the input shell starts with them up, and a UI that comes up on a pre-game screen must take
    /// them away before any key reaches them.
    session_maps: Option<bool>,
    /// Did this frame's [`Self::route_input`] dispatch an event that
    /// the input-event dispatcher produced inside a `WM_KEYDOWN`?
    ///
    /// The client's processing-key-down state, carried from the message to the frame that dispatches
    /// its action. [`Self::frame`] re-enters the original action-listener scope for the outbox drain
    /// on the strength of it, because the client's key-press global-message broadcast is synchronous
    /// and this shell's is a queue.
    key_down_dispatch: bool,
}

/// String-table enum used to resolve patch status text and the pre-game dialog captions below.
///
/// It is not the patch screen's private table: `CharacterManagementScreen`'s five dialogs and
/// `CharGenScreen`'s error dialogs all resolve their `StringInfo`s in the same one.
pub const PATCH_STRING_TABLE_ENUM: u32 = 0x1000_0002;

/// The table enum `GamePlayScreen`'s two logout confirmations build their
/// `StringInfo` values in. It is the only string-table enum in this build that is not
/// [`PATCH_STRING_TABLE_ENUM`].
pub const CLIENT_STRING_TABLE_ENUM: u32 =
    dereth_ui_screens::screens::gameplay::logout::STRING_TABLE_ENUM;

/// The phrase the delete-character dialog makes the user type back before it will
/// delete a character; the character-management screen owns it.
pub use dereth_ui_screens::screens::charmgmt::DELETE_CHARACTER_RESPONSE;

/// The connect-status text's two code states; the data-patch screen owns them.
pub use dereth_ui_screens::screens::datapatch::{CONNECT_TEXT_CONNECTED, CONNECT_TEXT_CONNECTING};

impl UiShell {
    /// Initialize the UI: build the element manager, register the element
    /// classes and the eight frameworks, and install the layout-enum resolver.
    ///
    /// `register_all` also performs UI initialization's last act — queueing mode
    /// `0x10000003` — so the shell comes back with the data-patch screen pending and no current
    /// screen, which is exactly the state the client is in before its first frame.
    ///
    /// # Errors
    /// [`UiShellError`] for either of the two dat objects without which no layout can be built.
    pub fn new(
        world: &Arc<dereth_dat::RetailDatStore>,
        display: (i32, i32),
    ) -> Result<Self, UiShellError> {
        use dereth_assets::Decode;
        use dereth_primitives::{AssetSource, DataId};

        // The screens are the later interface: everything they are built from -- layouts,
        // strings, fonts, art -- is read from its own files, and only the world's records (the
        // creation tables) and the pictures it names from the world's.
        let interface = world.interface_files();
        let store = &interface;

        // `MasterProperty 0x39000001` first: `LayoutDesc` cannot decode a property stream without
        // the id-to-type table.
        let master_id = DataId(0x3900_0001);
        let bytes = store
            .read(master_id)
            .map_err(|e| UiShellError::PropertyTypes(format!("{master_id:?}: {e}")))?;
        let master = dereth_assets::MasterProperty::decode_payload(master_id, &bytes)
            .map_err(|e| UiShellError::PropertyTypes(format!("{master_id:?}: {e}")))?;

        let mut ui = UiSystem::new(display);
        // Types *and* defaults: the enum attribute lookup falls back to the master row's
        // default, and the list-row highlight lives on that fallback.
        ui.install_master(&master);
        // The two services the text element reaches through singletons in the client and cannot
        // reach at all from `dereth-ui`: the string tables on the asset cache and font mapper.
        // Without them a label has no characters and no metrics, and there is no text anywhere
        // in the client.
        ui.strings = Some(Rc::new(crate::ui_draw::DatStringResolver::new(Arc::clone(
            store,
        ))));
        ui.fonts = Some(Rc::new(crate::ui_draw::DatFontProvider::new(Arc::clone(
            store,
        ))));
        let mut flow = UiFlow::new();
        dereth_ui_screens::register_all(&mut ui, &mut flow);

        // The mapper performs a two-level lookup. Never hard-code a
        // layout DataID: a DDD patch can move one.
        let assets: Rc<dyn AssetSource> = Rc::new(SharedStore(Arc::clone(store)));
        let resolver = DidMapperResolver::load_via_master(assets.as_ref())
            .map_err(|e| UiShellError::Resolver(e.to_string()))?;
        dereth_ui_screens::env::install_env(
            &mut ui,
            dereth_ui_screens::env::Env::new(Rc::clone(&assets), Rc::new(resolver)),
        );
        // The third service the UI element manager reaches through a singleton in the client, and
        // cannot reach at all from `dereth-ui`, is the asset database cache.
        // Tooltip creation builds its window from a layout inside the per-frame update, with no
        // caller to hand it an asset source. Without this every tooltip in the client is nothing
        // at all.
        ui.assets = Some(Rc::new(SharedStore(Arc::clone(store))));
        // A new UI starts with an empty request queue of its own: nothing a previous shell queued
        // can reach this one, which is the client's "the objects that would have made the calls
        // no longer exist".

        // The same two-level lookup as the layouts, on the string-table group. Never hard-code a
        // table DataID: a DDD patch can move one exactly as it can move a layout.
        let patch_strings = DidMapperResolver::load_group(
            assets.as_ref(),
            dereth_ui::framework::STRING_TABLE_GROUP,
        )
        .ok()
        .and_then(|r| {
            use dereth_ui::framework::LayoutEnumResolver as _;
            r.resolve(dereth_ui::framework::LayoutEnum(PATCH_STRING_TABLE_ENUM))
        });
        if patch_strings.is_none() {
            tracing::warn!("string-table enum {PATCH_STRING_TABLE_ENUM:#X} did not resolve");
        }
        // The same lookup on `GamePlayScreen`'s own table enum.
        let client_strings = DidMapperResolver::load_group(
            assets.as_ref(),
            dereth_ui::framework::STRING_TABLE_GROUP,
        )
        .ok()
        .and_then(|r| {
            use dereth_ui::framework::LayoutEnumResolver as _;
            r.resolve(dereth_ui::framework::LayoutEnum(CLIENT_STRING_TABLE_ENUM))
        });
        if client_strings.is_none() {
            tracing::warn!("string-table enum {CLIENT_STRING_TABLE_ENUM:#X} did not resolve");
        }

        // The two dat tables `CharGenScreen`'s pages are views over, through the **same**
        // two-level lookup: `DidMapper 0x25000000` entry 2 is `UNIQUEDB`, and its entries `0x0E`
        // and `4` are `CharGen_CharacterData` and `Weenie_SkillTable`. Hard-coding `0x0E000002`
        // would work against this dat build and break on any other.
        let mut stats = UiStats::default();
        let chargen_tables = load_chargen_tables(&SharedStore(Arc::clone(world)), world);
        let classic_creation = chargen_tables
            .as_ref()
            .map(|tables| {
                Rc::new(
                    dereth_classic_ui::panels::pregame::data::CreationData::load(
                        Rc::clone(&tables.world),
                        world,
                    ),
                )
            })
            .ok_or_else(|| "World creation tables unavailable".to_owned());
        if chargen_tables.is_none() {
            stats.chargen_table_failures += 1;
            tracing::warn!("the char-gen tables did not load; creation is unavailable");
        }

        Ok(Self {
            ui,
            interface: Arc::clone(store),
            flow,
            transitions: Vec::new(),
            last_host: HostState::default(),
            patch_strings,
            client_strings,
            character_actions: Vec::new(),
            chargen_actions: Vec::new(),
            chargen_tables,
            classic_creation,
            last_char_set: None,
            client_dir: std::path::PathBuf::from("."),
            movie_bytes: |_| None,
            movie: None,
            movie_frame: None,
            movie_audio_cue: None,
            sound_requests: Vec::new(),
            movie_stats: MovieStats::default(),
            log_off_requested: false,
            save_keymap_requested: false,
            stats,
            mouse_events: Vec::new(),
            target_mode_active: false,
            text_mode: false,
            focused_maps: Vec::new(),
            active_maps: Vec::new(),
            mode_maps: Vec::new(),
            session_maps: None,
            key_down_dispatch: false,
        })
    }

    /// The Classic presentation of this shell's already loaded creation rules.
    ///
    /// # Errors
    /// The world creation tables could not be loaded when this shell was constructed.
    pub fn classic_creation_data(
        &self,
    ) -> Result<Rc<dereth_classic_ui::panels::pregame::data::CreationData>, String> {
        self.classic_creation.clone()
    }

    /// Main-loop step 7: advance the UI element manager.
    ///
    /// The order inside is `UiFlow::frame`'s, which is the seven per-frame update steps, then the
    /// queued deliveries, then — **last** — the mode switch. Running the switch last is not a detail:
    /// the mode switch destroys the current screen, and a message-3 listener that ticked afterwards
    /// would tick a screen that is gone.
    ///
    /// Returns the [`UiRequest`]s the host must act on, including login and character requests.
    pub fn frame(
        &mut self,
        now: LocalTime,
        host: &HostState,
        input: &mut dyn UiInput,
    ) -> Vec<UiRequest> {
        self.frame_inner(now, host, input, &mut None)
    }

    /// App's synchronous external-listener/host boundary. The standalone wrapper retains its
    /// request-returning contract; App completes each action's subscribers before the next one.
    pub fn frame_with_dispatch(
        &mut self,
        now: LocalTime,
        host: &HostState,
        input: &mut dyn UiInput,
        dispatch: &mut DispatchHook<'_>,
    ) -> Vec<UiRequest> {
        self.frame_inner(now, host, input, &mut Some(dispatch))
    }

    /// Dispatch one normalized host message without advancing the screen or repeat clock.
    pub fn message_with_dispatch(
        &mut self,
        now: LocalTime,
        input: &mut dyn UiInput,
        dispatch: &mut DispatchHook<'_>,
    ) {
        self.sync_text_mode(input, false);
        let position = input.mouse_pos();
        self.ui.mouse_move(now, position.0, position.1);
        let mut dispatch = Some(dispatch);
        self.route_input(now, input, &mut dispatch, false);
        self.deliver_characters(input);
        let key_scope = self.key_down_dispatch;
        if key_scope {
            input.begin_action_dispatch(true);
        }
        self.deliver_pending(&mut dispatch);
        self.sync_text_mode(input, false);
        if key_scope {
            input.end_action_dispatch();
        }
    }

    /// Mirror focus after a synchronous host-owned editor or dialog operation.
    pub fn sync_input_scope(&mut self, input: &mut dyn UiInput) {
        self.sync_text_mode(input, false);
    }

    /// Re-register this interface's scopes after its inactive period.
    pub fn resume_input(&mut self, input: &mut dyn UiInput) {
        self.mode_maps.clear();
        self.focused_maps.clear();
        self.active_maps.clear();
        self.session_maps = None;
        self.text_mode = false;
        self.sync_text_mode(input, false);
    }

    fn frame_inner(
        &mut self,
        now: LocalTime,
        host: &HostState,
        input: &mut dyn UiInput,
        dispatch: &mut Option<&mut DispatchHook<'_>>,
    ) -> Vec<UiRequest> {
        self.apply_host_notices(host);
        if let Some(m) = self.push_host_state_into_screen(host) {
            self.queue(m);
        }
        self.last_host = host.clone();

        let before = self.flow.current_mode();

        // `UiFlow::frame` is `dereth-ui`'s convenience composition of the same four calls, and it
        // drains the deliveries and then switches. It cannot be used here, because the screens
        // do not queue a UI mode directly — they **emit a `UiRequest`** into a queue the host
        // drains (`dereth_ui_screens::requests`, and its own documentation says why: an element
        // handler is given only `&mut UiSystem`). So the request queue has to be drained *between*
        // the deliveries and the mode switch, or every transition a screen asks for is a frame late.
        //
        // In the client there is no gap at all: a screen handler queues a mode synchronously, and
        // the mode switch runs later in the same global-message-3 broadcast. This preserves
        // that order, with the queue spliced in where the client's direct call is.
        // Text updating calls `GetCaretBlinkTime` after its focus
        // guard on every text tick. There can be only one focus element, so refreshing the public
        // UI interval once immediately before the global-message-3 pass preserves the observable
        // cadence without making the platform-independent UI crate call USER32.
        self.ui.caret_blink_time =
            dereth_client_runtime::platform::caret::caret_blink_time_seconds();
        self.ui.use_time(now, input.as_pump());
        // The input manager's own per-frame update is step 6 above and it is what produced the frame's
        // input; the client dispatches each event from inside the input-event dispatcher, so the element
        // messages a click raises exist **before** the deliveries below drain. Putting the
        // dispatch anywhere later makes every screen answer a click one frame after it happened.
        self.route_input(now, input, dispatch, true);
        // Character updating runs the input-handler
        // list for every character that survived its three gates, and the only handler that wants
        // one is the focused text element's character handler — registered by
        // its focus-message arm (`0x2F`) and unregistered when
        // focus is lost, which is exactly [`dereth_ui::UiSystem::character`]'s own gate. Immediately
        // after `route_input`, because that is where the *action* half of the same key press went
        // and a character must not arrive a frame after the backspace next to it.
        self.deliver_characters(input);
        // A media step raised `PlayMovie`, or the movie
        // already running needs its next frame. Before the deliveries, because a movie that ends
        // this frame posts element message `0x1000000D` and the intro must hear it now rather than
        // next frame.
        self.tick_media(now);
        // **The second half of the action-to-listeners scope.**
        //
        // Key-press dispatch broadcasts global message 1 with the action
        // **synchronously**, so in the client every listener a key press wakes — including
        // chat-entry activation, the one thing in
        // the shipped build that a *key* can use to focus a text field — runs inside the
        // `WM_KEYDOWN`, inside the wrapper, where turning text mode on can arm the ignore-next-character latch.
        //
        // Here that broadcast goes into `UiSystem`'s **outbox** and is drained one step below
        // [`Self::route_input`]. So the scope is re-entered for the drain, and only when this
        // frame's dispatch actually came from a key down: with no key-down context there is
        // nothing to restore, and an unconditional mirror here would also move the shipped
        // char-gen edge off the late mirror below.
        let key_scope = self.key_down_dispatch;
        if key_scope {
            input.begin_action_dispatch(true);
        }
        self.deliver_pending(dispatch);
        if key_scope {
            // The poll standing in for the element's own synchronous text-mode switch, still inside
            // the wrapper — this is the call that arms the latch in a running client.
            self.sync_text_mode(input, false);
            input.end_action_dispatch();
        }
        // Run the flow's tick hook. `dereth-ui` drives it every frame because several
        // screens use it as their per-frame update hook.
        self.flow
            .update(&mut dereth_ui::framework::ScreenCx::new(&mut self.ui), now);

        let mut out = self.drain_requests();
        // The character screens' player-session operations and the selected-avatar mirror: the
        // screen flushes them into the queue here, after the frame's deliveries and before the
        // mode switch, and this drain routes them.
        if let Some(s) = self.flow.current_mut() {
            s.flush_to_host(&mut dereth_ui::framework::ScreenCx::new(&mut self.ui));
        }
        out.extend(self.drain_requests());

        // **Last**, and it must stay last: the mode switch destroys the current screen, and a
        // message-3 listener that ticked afterwards would tick a screen that is gone.
        self.flow
            .on_tick(&mut dereth_ui::framework::ScreenCx::new(&mut self.ui));
        self.note_switch(before);

        // A request emitted by a screen that was constructed during the mode switch — the new
        // screen's own `create` can queue one — is drained here rather than left for next frame.
        out.extend(self.drain_requests());
        // …and so is a `PlayMovie` the incoming screen's own state change raised, for the same reason.
        self.tick_media(now);

        // **The second mirror, and it is not a duplicate of the first.**
        //
        // Retail has no polling window at all: the text element's `0x2F`
        // arm turns text mode on **synchronously** as focus is gained, inside the focus take.
        // This shell polls instead, and the poll above runs at the character handler's own point
        // in the frame — immediately after `route_input`, where the *action* half of the same key
        // press went. Everything that takes focus from a **screen** rather than from a pointer
        // runs later than that: `flow.deliver`, `flow.update` and the mode switch are all below it.
        //
        // The shipped case is character generation. The name box taking focus is reached from
        // `chargen.rs`'s `summary_page_update`,
        // `Screen::update` and from `on_element_message` via `set_progress_state` / `do_random` —
        // both below the first mirror. With only that mirror, text mode would follow focus a
        // whole frame late and **every `WM_CHAR` in between would be discarded by the character
        // update's own text-mode gate** before this shell could see it. The gate is at message
        // time because the client's is, so a late mirror does not delay a character, it destroys
        // it: the first character typed into the name box would be lost.
        //
        // Here, and not one line earlier, so a screen constructed by the mode switch — whose `create`
        // can focus its own field — is included too.
        self.sync_text_mode(input, true);
        out
    }

    /// Complete messages raised by a non-element UI owner (e.g. Hud's vendor menu refill)
    /// without ticking, updating, switching mode, or replaying any raw input.
    pub(crate) fn deliver_with_dispatch(&mut self, dispatch: &mut DispatchHook<'_>) {
        self.deliver_pending(&mut Some(dispatch));
    }

    fn deliver_pending(&mut self, dispatch: &mut Option<&mut DispatchHook<'_>>) {
        if let Some(hook) = dispatch.as_deref_mut() {
            hook(self, UiDispatch::Requests);
        }
        // A callback can itself broadcast. Finish those descendants before returning to the
        // next raw action; do not tick/update the UI or change the deferred mode-switch step.
        loop {
            let deliveries = self.ui.drain_outbox();
            if deliveries.is_empty() {
                break;
            }
            for delivery in &deliveries {
                self.flow.deliver(
                    &mut dereth_ui::framework::ScreenCx::new(&mut self.ui),
                    delivery,
                );
                if let Some(hook) = dispatch.as_deref_mut() {
                    hook(self, UiDispatch::Requests);
                }
            }
            if dispatch.is_none() {
                break;
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // The mouse reaches the tree
    // -----------------------------------------------------------------------------------------

    /// The UI element manager's two input-handler registrations and its map-3 action callback, in the
    /// order the client reaches them.
    ///
    /// [`dereth_ui::UiSystem::mouse_move`], [`mouse_leave`](dereth_ui::UiSystem::mouse_leave),
    /// [`mouse_down`](dereth_ui::UiSystem::mouse_down) and
    /// [`mouse_up`](dereth_ui::UiSystem::mouse_up) are `dereth-ui`'s mouse-move, mouse-leave,
    /// mouse-down and mouse-up handlers, with hit testing, capture, hover and drag inside them.
    /// This is what calls them; without it no pointer event reaches an element and every button
    /// is inert to a real click, even though a test that broadcasts the element message straight
    /// onto the tree would pass.
    ///
    /// The order is the client's:
    ///
    /// 1. **`WM_MOUSELEAVE`** → the mouse-leave event, which sets the pointer-has-left flag and
    ///    suppresses all hit testing until the pointer comes back;
    /// 2. **the move**, from the input poll's step 3. Before the buttons, so the last-entered element
    ///    and the mouse-over top element are this frame's before a press is judged against them — which is
    ///    what the button-dispatch "return if the pointer left" gate reads;
    /// 3. **the actions**, through the UI manager's switch:
    ///
    ///    | action | press | release |
    ///    |---:|---|---|
    ///    | 4 | consumed, no-op | consumed, no-op |
    ///    | 5–0x0C | mouse down (action, extent) | mouse up (action, callback) |
    ///    | 0x0D–0x0F | mouse down (the taps) | consumed, no-op |
    ///    | anything else | declined | declined |
    ///
    ///    "so the UI's button number *is* the action id: 5 = wheel up, 6 = wheel down, 7 = left,
    ///    8 = right, 9 = middle, 10/11/12 = left/right/middle double-click, 0x0D–0x0F = spare
    ///    buttons".
    ///
    /// **The coordinates come from input-manager state, not the message.** Mouse-down dispatch
    /// first reads that state, and in mouse-look its stored mouse position is the only truthful answer
    /// because the OS cursor is warped to the screen centre every frame.
    ///
    /// **The wheel is not this seam's, and the manager action handler's cases 5 and 6 are dead code in the
    /// shipped build.** The wheel is `DIMOFS_Z[+]`/`[-]` in input map **0xA**, and map 0xA is
    /// pushed by the scrollable element at `priority::FOCUSED_UI` with *itself* as the callback
    /// (`dereth_input::RETAIL_MAP_REGISTRATIONS`), so actions 5 and 6 reach that element's own
    /// action handler and never the manager's. Map 3 binds no wheel control at all. A scrollable that
    /// declines falls through to the handler list, which emits a key-press event rather than a mouse-down event.
    ///
    /// **The handler-list leg is here too** — step 4 below, and [`Self::action_handler`] for why.
    /// It matters even with no focused text element: the key-press event's *whole point* is the
    /// global-message-1 broadcast it makes when **nothing** is focused, and nineteen listeners in
    /// this build are registered for exactly that broadcast. Without it every hotkey in the game
    /// screen is inert.
    ///
    /// **Character delivery follows**, as [`Self::deliver_characters`], called by [`Self::frame`]
    /// immediately after this function. The input manager's text-mode flag gates character
    /// delivery entirely and is turned on by a focused text element.
    fn route_input(
        &mut self,
        now: LocalTime,
        input: &mut dyn UiInput,
        dispatch: &mut Option<&mut DispatchHook<'_>>,
        global_loop: bool,
    ) {
        // Reset per frame; set below by any event the input-event dispatcher produced
        // inside a `WM_KEYDOWN`. [`Self::frame`] reads it to re-enter the same key-down scope for
        // the outbox this dispatch fills — see there.
        self.key_down_dispatch = false;
        if dispatch.is_some() {
            self.deliver_pending(dispatch);
        }
        // 1.
        if input.take_mouse_left_window() {
            self.ui.mouse_leave();
        }
        // 2. -- the hit test that records the element under the pointer.
        if let Some((x, y)) = input.take_mouse_move() {
            self.ui.mouse_move(now, x, y);
        }
        // 2b. The UI manager's global-message-3 broadcast, which the smart-box wrapper handles as
        //     its global loop. Three things about it are load-bearing:
        //
        //     * it is **unconditional** -- the broadcast is a statement of the per-frame update, not
        //       something a pointer movement raises, so a still pointer searches too;
        //     * it is **after** the hit test above, because the mouse update is the statement
        //       before tooltip checking and the global broadcast in the manager update, and
        //       object lookup reads the element-under-pointer state that the hit test just wrote;
        //     * it is **before** input-manager time advancement, the manager update's last-but-one
        //       statement and the one that dispatches this frame's clicks -- so the
        //       mouse-over search-reason pick is always armed first and the frame's own click arm
        //       overwrites it (mouse down gates on search reason below examine, mouse up on `< 3`,
        //       and the global-loop value 1 passes both).
        //
        //     The global loop runs only while the search reason is below 1, and the smart-box
        //     object-found notice restores the reason, so
        //     it passes on every in-game frame. Nothing on that path touches the network: the
        //     object search writes four globals and either sets or clears the selection cursor.
        if global_loop {
            if let Some(hook) = dispatch.as_deref_mut() {
                hook(self, UiDispatch::Hover(input.mouse_pos()));
            }
        }
        if dispatch.is_some() {
            self.deliver_pending(dispatch);
        }
        // 3. The position is re-read per event, not hoisted: original mouse-down dispatch first
        //    reads input-manager mouse state, and a handler this dispatch runs can move it.
        let mut declined = Vec::new();
        for e in input.take_actions() {
            // This client's own actions (the performance panel's key) belong to no interface:
            // they go past the screens to the runtime.
            if e.input_map == dereth_input::dereth::INPUT_MAP {
                declined.push(e);
                continue;
            }
            let (x, y) = input.mouse_pos();
            // Action dispatch has a three-line wrapper around everything below, and its only
            // observable is a copy of key-down-in-progress into the action-dispatch latch; without
            // it text-mode switching never sees `true` and the ignore-next-character latch is never
            // armed. The bracket is per **event**, not per frame, because the client wraps each
            // input event.
            //
            // **This bracket is unfalsifiable in this build, and it is kept anyway.** Mutating the
            // argument to a constant `false` leaves the ignore-next-character tests green: every
            // handler a key press reaches here answers through `UiSystem`'s **outbox**, so the
            // focus that arms the latch is taken in the second bracket below, not in this one. It
            // is the original shape
            // (the listener dispatch wraps every action callback) and the first screen that takes
            // focus synchronously from an action handler will need it, so it stays.
            self.key_down_dispatch |= e.from_key_down;
            input.begin_action_dispatch(e.from_key_down);
            // 3a. The *winning map's* callback gets first refusal. When a mode
            //     owns a higher-priority map than the UI element manager, the mode receives the
            //     action first. Intro, credits and character management use this route.
            //     `Some(false)` is the map owner *declining*,
            //     which in the client reaches the registered handler list and never invokes the
            //     manager, because the manager is not the callback of a map it lost. That is why
            //     a declined event skips straight to step 4 here.
            let mut give_back = false;
            if let Some(consumed) = self.mode_on_action(&e) {
                if !consumed {
                    self.action_handler(&e, x, y);
                    give_back = true;
                }
            } else if !Self::reaches_the_managers_on_action(&e)
                || !self.on_action(&e, x, y, dispatch)
            {
                // 4. The action dispatcher runs the registered handler list for everything the
                //    winning map's callback declined, and UI initialization registers the manager
                //    there with flag `0x11`. That is the *only* road to the key-press event.
                //
                // The `declined.push` must stay conditional: if an event the handler list
                // *consumed* went back into the queue, `take_actions` would hand it out again on
                // the **next** frame, and the one after that, for ever — the first Enter in a
                // text box would re-fire once per frame, closing the chat box the instant a player
                // put the focus back.
                //
                // The original dispatcher invokes the handler list only when the map callback
                // declines, and a handler that returns true ends the walk; what is put
                // back here is what nobody took, which is what the name says.
                if !self.action_handler(&e, x, y) {
                    give_back = true;
                }
            }
            // **This is where the latch is actually armed.** The text element's
            // `0x2F` arm enables text mode *synchronously* from inside the handler this dispatch
            // just ran, so the write lands between the original listener dispatch's two steps. This shell polls
            // instead ([`Self::sync_text_mode`]), and the poll therefore belongs **inside** the
            // bracket and **per event**, so a focus taken by event `k` is mirrored before event
            // `k+1` is dispatched, exactly as a separate input event would.
            //
            // [`Self::deliver_characters`] still polls immediately below this loop, for everything
            // that takes focus without an action at all — a mouse click, or intro-screen
            // construction — and it is a no-op when this call already moved the edge.
            if dispatch.is_some() {
                self.deliver_pending(dispatch);
            }
            self.sync_text_mode(input, false);
            input.end_action_dispatch();
            if give_back {
                declined.push(e);
            }
        }
        input.put_back_unconsumed(declined);
    }

    /// Character delivery and the text-mode write
    /// that decides whether it ever has anything to deliver.
    ///
    /// Two halves of one seam:
    ///
    /// * **text mode.** In the client the *element* turns it on: when focus is gained, the 0x2F arm
    ///   registers its character handler and enables text mode; when focus is lost, it unregisters
    ///   that handler, deselects the element, and disables text mode. `UiSystem` has no route to
    ///   the input manager — `dereth-ui`'s seam is deliberately that narrow — so the shell mirrors the
    ///   focus element into the input manager here instead. The observable is the same: text mode is
    ///   on exactly while an **editable** text element holds focus.
    /// * **the characters.** `take_characters` has already been through
    ///   the ignore-next-character, text-mode, and main-window-focus gates, so each one goes straight to
    ///   [`dereth_ui::UiSystem::character`], which preserves the original "the focus element
    ///   and nobody else" rule.
    ///
    /// **Selectable-but-not-editable text does not turn text mode on**: a label you can select and
    /// copy from is not a place characters go; the original character handler first checks the
    /// editable bit (`bitfield & 1`).
    fn deliver_characters(&mut self, input: &mut dyn UiInput) {
        self.sync_text_mode(input, false);
        for ch in input.take_characters() {
            let mut buf = [0u16; 2];
            for unit in ch.encode_utf16(&mut buf) {
                if self.mode_character(*unit) {
                    continue;
                }
                self.stats.characters_delivered += 1;
                self.ui.character(*unit);
            }
        }
    }

    /// **The decision mirrors, on its own so that a test can ask it.**
    ///
    /// The text element's focus-message arm (`0x2F`) is what the client uses,
    /// and its two gates are exactly the two terms here:
    ///
    /// * **an editable text element holds focus.** The arm's own outer test is
    ///   `(bitfield & 1) || (bitfield & 4)` — editable *or* selectable — but only the
    ///   `bitfield & 1` leg registers the character handler, whose first line checks the same bit.
    ///   A label you can sweep-select and copy from is not a
    ///   place characters go.
    /// * **the intro is up.** The original intro screen enables text mode during construction
    ///   and disables it during destruction, with no text element anywhere in its
    ///   layout: it is the one screen that turns the gate on for itself, and that is what "any key
    ///   skips the intro" is.
    ///
    /// It is `pub` and takes no window message on purpose. The decision must be testable without
    /// constructing the window system's own key event, which cannot be
    /// constructed in a test, so the one function deciding whether the player moved could have no
    /// test at all. This decision is reachable from a test without a window, a device or a
    /// frame.
    #[must_use]
    pub fn wants_text_mode(&mut self) -> bool {
        let editable = match self.ui.focus_element() {
            Some(h) => self
                .ui
                .text_element_mut(h)
                .is_some_and(|t| t.bits.editable()),
            None => false,
        };
        let intro = self.flow.current_mode() == Some(mode::INTRO);
        editable || intro
    }

    /// Synchronize text-mode state on the **edge**, separately from the focused text element's
    /// input maps. It has two call sites, one after input routing and one after the mode switch.
    ///
    /// `UiSystem` has no route to the input manager — `dereth-ui`'s seam is deliberately that narrow — so the
    /// shell mirrors [`Self::wants_text_mode`] into the input manager instead of the element doing it
    /// itself. Written on the edge so enabling text mode does not re-arm the original
    /// ignore-next-character latch every frame.
    ///
    /// Returns whether the mirror moved, so a caller counts edges rather than assuming one.
    ///
    /// `late` is only for [`UiStats::text_mode_edges_late`]; the write itself is identical.
    fn sync_text_mode(&mut self, input: &mut dyn UiInput, late: bool) -> bool {
        // **Text mode and the focused map set are two separate mirrors.**
        //
        // Text-mode switching and the focused element's input-map registration are two different
        // transitions in the client. Text mode is *editable text has focus, or the intro is up*;
        // the map set is *whatever the focused element's registration behavior pushes*. They
        // agree for an editable text box and disagree everywhere else — a single flag would leave
        // a focused list box or a selectable-only chat log registering nothing, with a dead
        // wheel. See [`Self::focused_input_maps`].
        // **The `priority::UNFOCUSED_UI` band, and it is mirrored first.**
        //
        // Activation registers the active element's maps at priority 0 and its activation alert
        // registers them at priority 2000, both **before** the tail restores the focus element and
        // thereby pushes the focused band.
        // Both bands are edges on the same frame, so their relative order here is the client's.
        //
        // **The pre-game screen's own band, and it is mirrored FIRST.** The three
        // original constructors register map 9 (and, for the intro, map 3) before the screen has built a
        // single element, so in the client this registration always precedes anything the same
        // screen's elements later push at `priority::FOCUSED_UI`. That ordering is the whole reason
        // a focused edit box still wins `DIK_RETURN`: `register` prepends within a band, so the
        // maps that go in *later* walk in front, and the text element's map 7 — which binds the
        // same `DIK_RETURN` to the same action `0x25` — is registered later and therefore outranks
        // map 9 for as long as the box has focus. Reverse these two blocks and that inverts.
        input.set_target_input_map(self.target_mode_active);
        let mode_maps = Self::mode_input_maps(self.flow.current_mode());
        if mode_maps != self.mode_maps {
            input.set_mode_input_maps(&mode_maps);
            self.mode_maps = mode_maps;
            self.stats.mode_map_edges += 1;
        }
        // **The character session's maps, which only the gameplay screen has.** Movement,
        // examine and use, the panel toggles, the quickbar, emotes, combat and chat are registered
        // when a character session begins and dropped when it ends; this build enters the
        // gameplay screen on that same edge and leaves it on the log-off or disconnect edge, so
        // "the gameplay screen is up" is the session. Mirrored beside the pre-game screen's own
        // band because the two are the two halves of one change: leaving character selection
        // for the world takes map 9 away and brings these up.
        let session = self.flow.current_mode() == Some(mode::GAME_PLAY);
        if self.session_maps != Some(session) {
            input.set_character_session_input_maps(session);
            self.session_maps = Some(session);
        }
        let active = self.active_input_maps();
        if active != self.active_maps {
            input.set_active_input_maps(&active);
            self.active_maps = active;
            self.stats.active_map_edges += 1;
        }
        let maps = self.focused_input_maps();
        let maps_moved = maps != self.focused_maps;
        if maps_moved {
            // The client's focus-change order unregisters the loser's maps and then registers the
            // gainer's maps, which is what
            // [`crate::input::InputShell::set_focused_input_maps`] does in one call.
            input.set_focused_input_maps(&maps);
            self.focused_maps = maps;
            self.stats.focused_map_edges += 1;
        }
        let want = self.wants_text_mode();
        if want == self.text_mode {
            return false;
        }
        self.text_mode = want;
        input.set_text_mode(want);
        self.stats.text_mode_edges += 1;
        if late {
            self.stats.text_mode_edges_late += 1;
        }
        true
    }

    /// **The focus manager's gain arm: which input maps the newly focused element registers, in
    /// the client's own registration order.**
    ///
    /// Two registration functions decide the whole set:
    ///
    /// The scrollable base registers map `0x0A` unconditionally. The text override calls that base
    /// first, then tests bit 1 to register the typing barrier at 2990 and map 7 at 3000, and tests
    /// bits 1 or 4 to register map 8 at 3000.
    ///
    /// So the gates are **per map and not uniform**, which is the reason this cannot be one
    /// `bool`:
    ///
    /// | map | gate | which elements |
    /// |---|---|---|
    /// | `0x0A` `ScrollableControls` | **none** | every scrollable control — scrollable, list, item list, text, button, menu, scrollbar |
    /// | `1` (`MAP_BLOCK_KEYBOARD`) and `7` | `bitfield & 1` — editable | an editable text element |
    /// | `8` | `bitfield & 5` — editable **or** selectable | that, plus a selectable label such as the chat log |
    ///
    /// Everything outside the scrollable subtree uses the base registration, which registers only
    /// the element's own input-map id (attribute `0x4E`) and never `0x0A`.
    ///
    /// [`dereth_ui::UiSystem::takes_focus_on_press`] is the subtree test, and `TextElement::bits`
    /// carries the two client bitfield flags
    /// (`0x0001` editable, attribute `0x16`; `0x0004` selectable, attribute `0x27`).
    ///
    /// **There is no intro fallback.** In retail the intro enables text mode and has no text
    /// element at all, so answering `FOCUSED_TEXT_MAPS` whenever the intro is up and nothing has
    /// focus would register maps the client never does. It would also be wrong in effect: maps
    /// **7** and **9** both bind an unmodified `DIK_RETURN` to action `0x25` and both sit at 3000,
    /// so the intro's own map 9 would have to out-race four maps that no element of the intro
    /// layout could ever register, and which one won would be decided by the order of two mirrors
    /// rather than by the client. The real producer is
    /// [`PREGAME_MODE_INPUT_MAPS`]; `wants_text_mode` keeps its intro term, because enabling text
    /// mode is the *character* path.
    ///
    /// The order is the **registration** order, not the walk order, and the walk depends on two
    /// things: `dereth_input::dispatch::InputMapStack::register` prepends an equal-priority newcomer,
    /// and `crate::input::focused_map_priority` puts map 1 at **2990**, where the other three go in at
    /// 3000. So `[0x0A, 1, 7, 8]` walks as **`8, 7, 0x0A, 1`**, the barrier last.
    ///
    /// With the barrier at 2990, `ScrollableControls`' `Ctrl+DIK_UP`/`DOWN` are not behind the
    /// barrier while a text box has focus: those two keys reach `ScrollableControls` and scroll the focused element whether or not it is an
    /// editable box, which is the same answer a focused list box already gave (it registers
    /// `[0x0A]` alone and has no barrier at all). The wheel is a mouse control and passes map 1
    /// wherever it sits.
    #[must_use]
    pub fn focused_input_maps(&mut self) -> Vec<(u32, i32)> {
        use crate::input::focused_map_priority;
        use dereth_input::dispatch::priority;

        let mut maps = Vec::new();
        let Some(h) = self.ui.focus_element() else {
            return maps;
        };
        // **The base call, and it is FIRST.** Every focus-gain override in the client begins by
        // calling its parent, and the chain bottoms out by registering the ancestors' input maps
        // one priority lower per generation, then the element's own at the priority it was
        // handed. This is the **third** call site of that registration, beside activation's 0 and
        // the activation alert's 2000.
        //
        // It is not empty in the shipped data, which is what separates it from the other two.
        // Attribute `0x4E` is on 2 of 2 162 elements and **both are leaf edit fields** — one under
        // the `Dialog` layout `0x2100003C`, which every dialog in the client is built from — so
        // this arm fires whenever one of those boxes takes focus, and it registers map **9**
        // (`DialogBoxes`). Neither is a root element, which is exactly why the *other* two sites
        // stay empty: activation only ever reaches a root and walks up, never down.
        maps.extend(self.ui.input_maps_for_registration(h, priority::FOCUSED_UI));
        if self.ui.takes_focus_on_press(h) {
            maps.push((crate::input::SCROLLABLE_INPUT_MAP.0, priority::FOCUSED_UI));
        }
        let (editable, selectable) = self
            .ui
            .text_element_mut(h)
            .map_or((false, false), |t| (t.bits.editable(), t.bits.selectable()));
        if editable {
            maps.push((1, focused_map_priority(1)));
            maps.push((7, focused_map_priority(7)));
        }
        if editable || selectable {
            maps.push((8, focused_map_priority(8)));
        }
        maps
    }

    /// **The two input-map registrations made on the activated window: the
    /// `priority::UNFOCUSED_UI` band.**
    ///
    /// Activation first registers the element's maps at priority 0. Its activation alert then
    /// registers the same maps at priority 2000. Thus the maps are in the stack **twice**, and
    /// deactivation's single unregister call takes both because unregistration never compares the
    /// priority. The order here
    /// is the client's registration order: the `priority::LOWEST` copy first.
    ///
    /// The per-element content is
    /// [`dereth_ui::UiSystem::input_maps_for_registration`] — the element's input-map id plus each ancestor's,
    /// one priority lower per generation — and that page carries the census and the reason this is
    /// **empty for every activatable element in the shipped data**. Two consequences worth stating
    /// where a reader will meet them:
    ///
    /// * **no shipped binding changes hands.** A band that registers nothing cannot shadow
    ///   anything, so the walk a focused text box produces is still `8, 7, 0x0A, 1` and
    ///   `priority::GAMEPLAY`'s movement maps still sit directly below it.
    /// * **the other two maps at 2000 in retail have distinct owners.** `CameraManager`'s map 6
    ///   is mirrored by `App::apply_world_camera_action`; UI input map `0x1000000B`
    ///   (`TargetedUsage`) by `InputShell::set_target_input_map`. See
    ///   [`dereth_input::dispatch::priority::UNFOCUSED_UI`] for the three-producer census.
    #[must_use]
    pub fn active_input_maps(&self) -> Vec<(u32, i32)> {
        use dereth_input::dispatch::priority;
        let Some(h) = self.ui.active_element() else {
            return Vec::new();
        };
        let mut out = self.ui.input_maps_for_registration(h, priority::LOWEST);
        out.extend(
            self.ui
                .input_maps_for_registration(h, priority::UNFOCUSED_UI),
        );
        out
    }

    /// **The current mode's own input-manager map registrations** — [`PREGAME_MODE_INPUT_MAPS`]
    /// looked up, in the client's registration order.
    ///
    /// An associated function rather than a method so that a test can ask it about a mode without
    /// standing a screen up, and so that the eight modes with no registration answer the empty
    /// vector rather than being absent from the question. `None` — no screen — is the same empty
    /// answer, which is the destructor state.
    #[must_use]
    pub fn mode_input_maps(mode: Option<UiMode>) -> Vec<u32> {
        let Some(m) = mode else { return Vec::new() };
        PREGAME_MODE_INPUT_MAPS
            .iter()
            .find(|(mm, _)| *mm == m)
            .map_or_else(Vec::new, |(_, maps)| maps.to_vec())
    }

    /// What [`Self::mode_input_maps`] last mirrored into the input manager, so a test can read the
    /// live state rather than recompute the model.
    #[must_use]
    pub fn registered_mode_maps(&self) -> &[u32] {
        &self.mode_maps
    }

    /// The current mode's own action callback for the maps it registered above
    /// UI element manager.
    ///
    /// Input dispatch invokes the **winning map's callback** and walks the registered handler list
    /// only if that returns false. All three
    /// pre-game screens are that callback for input map 9 — see [`PREGAME_MODE_INPUT_MAPS`] for
    /// the four registration sites — so all three arrive here, and each has its own body:
    ///
    /// The intro declines release edges, queues mode `0x1000000A` for action `0x27`, and otherwise
    /// advances one media state. Credits never reads the event: it unregisters global message 3,
    /// shows the please-wait dialog, queues mode `0x1000000A`, and returns true. Character management
    /// shows the confirm-exit dialog and returns true only for action `0x27`.
    ///
    /// Three differences between them are load-bearing and all three are reproduced. Only the
    /// **intro** tests the start flag, so on the intro a press-and-release is one advance while on the
    /// other two the release edge is answered as well (harmlessly: the credits' `finished` and
    /// the confirm-exit dialog's existing-context guard each refuse the
    /// second). Only **character management** tests the action, so Enter there returns `false` and
    /// falls through to the registered handler list exactly as it does in retail. And the **credits**
    /// consume everything map 9 carries, because the body never looks at the event at all.
    ///
    /// # Why not global message 1
    ///
    /// Reaching credits and character management through the global-message-1 broadcast instead
    /// would **not** be observationally equivalent. Global message 1 is broadcast for *any*
    /// action the focused or active element declined, from *any* input map — so on the credits
    /// screen, which has no focusable element at all, **every bound key would end the roll**: `W`
    /// (`MovementForward`, map 4), `F5` (`ToggleSpellbookPanel`, `UICommands`), a quickbar digit.
    /// Retail ends the roll only for what map 9 carries, because listener dispatch gives first
    /// refusal to the winning map's callback and the credits screen is the callback of map 9
    /// alone.
    fn mode_on_action(&mut self, e: &dereth_input::InputEvent) -> Option<bool> {
        // **The map-9 arm of this filter depends on its producer.** The intro's entry is `[9, 3]`
        // because the intro registers both; without the map-9 registration `DIK_RETURN` on the
        // intro would resolve in `ChatCommands` (`0x1000000A`) to `EnterChatMode` and be refused
        // here. See [`PREGAME_MODE_INPUT_MAPS`], which is the registration; the tests assert both
        // directions: a map-9 press gets through, and an event on a map the intro does not own
        // does not.
        //
        // The table is consulted rather than three `if`s, so that the set of modes with an
        // action handler and the set of modes that register a map cannot drift apart: a screen that
        // registers map 9 and is not answered here would be registered and never answered.
        let m = self.flow.current_mode()?;
        let maps = PREGAME_MODE_INPUT_MAPS
            .iter()
            .find(|(mm, _)| *mm == m)
            .map(|(_, s)| *s)?;
        if !maps.contains(&e.input_map.0) || e.input_map.0 == CHARACTER_SCREEN_SCROLL_MAP {
            return None;
        }
        // The screen's own action callback: the intro's (release edges declined), the credits'
        // (both edges end the roll) and character management's (Enter declined, so it reaches the
        // handler list). A mode in the table whose screen has no callback answers `None`, which
        // puts the event back on the road it would have taken anyway.
        let screen = self.flow.current_mut()?;
        let a = screen.on_mode_action(&mut dereth_ui::framework::ScreenCx::new(&mut self.ui), e)?;
        if a.handled {
            match m {
                mode::INTRO => self.stats.intro_actions += 1,
                mode::CREDITS => self.stats.credits_actions += 1,
                mode::CHARACTER_MANAGEMENT => self.stats.charmgmt_actions += 1,
                _ => {}
            }
        }
        if let Some(q) = a.queue {
            self.queue(q);
        }
        Some(a.consumed)
    }

    /// Handles an intro character the way the client reaches it: through the
    /// character handler that construction registers with flag `0x02` (the **character** list).
    ///
    /// Returns whether the intro took the character; the focused-text delivery is skipped when it
    /// did, because in the client the intro's handler and a text field's are two entries in the
    /// same list and the intro is the only one registered while it is up.
    fn mode_character(&mut self, ch: u16) -> bool {
        if self.flow.current_mode() != Some(mode::INTRO) {
            return false;
        }
        let queued = {
            let Some(screen) = self.flow.current_mut() else {
                return false;
            };
            let Some(queued) = screen
                .on_mode_character(&mut dereth_ui::framework::ScreenCx::new(&mut self.ui), ch)
            else {
                return false;
            };
            queued
        };
        self.stats.intro_characters += 1;
        if let Some(m) = queued {
            self.queue(m);
        }
        true
    }

    /// the handler-list leg has this exact order:
    ///
    /// ```text
    /// on a press, emit the key-press event and return;
    /// otherwise, while debug-console input is inactive,
    /// offer the action to the focused element (or the active element when nothing is focused),
    /// then broadcast global message 2 with the action if the element declines it.
    /// ```
    ///
    /// and the key-press event is the same shape with **global message 1** and a trailing
    /// visibility-toggle action. [`dereth_ui::UiSystem::key_press`] is that function; it had **no
    /// caller anywhere in this workspace, tests included**, which is why every keyboard gesture in
    /// the game screen was inert: the eighteen quickbar hotkeys, the make-shortcut key, both logout
    /// arms and the UI-lock toggle of `GamePlayScreen::handle_key_press`, `Esc` on the stack-size
    /// box, the char-gen wizard's and the epilogue's registrations — all of them listen on global
    /// message 1, and nothing raised it.
    ///
    /// **This runs for every event `on_action` declined, not only keyboard ones**, because the
    /// client's handler list is not filtered by input map: the map callback gets first refusal and
    /// the handlers get whatever it did not take. In this build the point is moot — the left, right
    /// and middle buttons are bound only in map 3 (`tests/routing.rs`), so a mouse action is always
    /// consumed above — but the shape is the client's rather than a keyboard special case.
    ///
    /// The debug console's input-active flag has no counterpart in this build; there is no
    /// debug console, so the gate is the constant it evaluates to.
    ///
    /// Returns whether the focused or active element consumed it.
    fn action_handler(&mut self, e: &dereth_input::InputEvent, x: i32, y: i32) -> bool {
        let ev = dereth_ui::focus::InputEvent {
            action: e.action.0,
            start: e.start,
            x,
            y,
        };
        // The container leg of action dispatch, for the containers
        // that live on a `Screen` rather than in the arena. `dispatch_action` asks every ancestor
        // **behaviour** already; `MainChat` and `FloatingChat` are `GamePlayScreen` fields,
        // so they are asked here, in the same slot in the order — *before* the focused element's
        // own `on_action`, because a container chains to its base first
        // and only then runs its own `switch`. Getting that backwards is the difference between
        // Enter sending the line and Enter merely dropping focus.
        if let Some(target) = self.ui.focus_element().or_else(|| self.ui.active_element()) {
            if self.screen_on_child_action(target, &ev) {
                self.stats.child_actions_consumed += 1;
                return true;
            }
        }
        if e.start {
            self.stats.key_presses += 1;
            // `key_press` broadcasts global 1 itself when nobody consumed, and returns whether
            // it did not have to.
            let consumed = self.ui.key_press(&ev);
            if !consumed {
                self.stats.key_presses_broadcast += 1;
            }
            return consumed;
        }
        let target = self.ui.focus_element().or_else(|| self.ui.active_element());
        if let Some(h) = target {
            if self.ui.dispatch_action(h, &ev) {
                return true;
            }
        }
        self.stats.key_releases_broadcast += 1;
        self.ui
            .broadcast_global(dereth_ui::msg::global::KEY_UP_UNCONSUMED, ev.action);
        false
    }

    /// [`dereth_ui::framework::Screen::on_child_action`] on the screen that is up.
    fn screen_on_child_action(
        &mut self,
        child: dereth_ui::ElemHandle,
        ev: &dereth_ui::focus::InputEvent,
    ) -> bool {
        let Some(screen) = self.flow.current_mut() else {
            return false;
        };
        screen.on_child_action(
            &mut dereth_ui::framework::ScreenCx::new(&mut self.ui),
            child,
            ev,
        )
    }

    /// Which events get first refusal from [`Self::on_action`], matching the UI manager's callback.
    ///
    /// This build has **one** action callback where the client has many: every map in
    /// [`crate::input::BASE_MAP_REGISTRATIONS`] and every map in
    /// [`crate::input::FOCUSED_TEXT_MAPS`] is registered under a `UiShell` callback, so the shell
    /// has to decide, per map, *which* of the client's callbacks it is standing in for. The base
    /// rule is one line — `e.input_map == UI_INPUT_MAP` — and it is right for almost everything: the element manager is the callback for map **3** (`MouseCommands`) and `0x0D`
    /// only, so an event carrying map 4 or `0x10000002` belongs to player-state or combat input
    /// and must fall through to the registered input-handler list, which is
    /// [`Self::action_handler`].
    ///
    /// **`0x0A` is the exception, and it is a declared deviation from the client's dispatch.**
    /// `ScrollableControls` is the only section of the shipped merged keymap that binds
    /// `DIMOFS_WHEEL`, and in retail its callback is the focused **element**:
    /// The scrollable registration passes the focused element as callback at priority 3000. That
    /// element's own action handler is its scrollable or text action handler,
    /// which switches on `0x16`..`0x28` only); both bubble through the child-action handler to a
    /// root with no parent and return **false**, after which the handler-list action dispatcher
    /// reaches a press arm that calls
    /// the key press with `(action, extent)` and **not** the mouse down. The consequence: **the
    /// producer for the scrollable element-message handler's wheel arm is not known**, and the
    /// obvious
    /// candidate is ruled out by five separate readings, which are kept in
    /// `dereth_ui::scrollable::Scrollable::wheel_target`.
    ///
    /// So there are two honest options and no third: leave the wheel inert, or choose a
    /// client-side route to the arm. This build chooses the route, **narrowly** — map `0x0A`, and
    /// only its two wheel actions — because the arm consumes element message `0x1C` with `dwParam1 ∈ {5, 6}`, and `0x1C` has exactly one
    /// producer in retail, which is what
    /// [`dereth_ui::UiSystem::mouse_down`] is. The observable is retail's: one detent scrolls the
    /// scrollable **under the pointer** by one scroll-delta step.
    ///
    /// What is deliberately *not* done: map `0x0A`'s other two bindings, `Ctrl+DIK_UP` and
    /// `Ctrl+DIK_DOWN`, resolve to the same actions 5 and 6 and are admitted here too — they are
    /// literally named `ScrollUp`/`ScrollDown` in the keymap and the client has no other route for
    /// them either — but they are keyboard controls, so `walk_input_maps` stops them at the
    /// barrier (map 1) whenever a text box holds focus, which is `0x0A`'s only registration
    /// condition in this build. They are therefore transcribed and, through this route,
    /// unreachable; said out loud rather than left to be discovered.
    ///
    /// **If retail's producer is ever found, this is the function to delete.** It is one
    /// predicate, it is named, and the wheel tests state what it buys.
    fn reaches_the_managers_on_action(e: &dereth_input::InputEvent) -> bool {
        if e.input_map == UI_INPUT_MAP {
            return true;
        }
        // Target mode forwards map B's left and right buttons to the manager after observing the
        // target-mode leave edge. It does not swallow them.
        if e.input_map == crate::input::TARGET_INPUT_MAP && matches!(e.action.0, 7 | 8) {
            return true;
        }
        e.input_map == crate::input::SCROLLABLE_INPUT_MAP
            && (e.action.0 == dereth_ui::focus::action::WHEEL_UP
                || e.action.0 == dereth_ui::focus::action::WHEEL_DOWN)
    }

    /// Implements the UI manager's input-action switch and nothing else. Returns whether the UI
    /// consumed the event.
    fn on_action(
        &mut self,
        e: &dereth_input::InputEvent,
        x: i32,
        y: i32,
        dispatch: &mut Option<&mut DispatchHook<'_>>,
    ) -> bool {
        use dereth_ui::focus::action;
        let a = e.action.0;
        if a == action::IGNORED {
            // `case 4:` on both edges — consumed and nothing happens.
            return true;
        }
        if e.start {
            // Actions 5..=15 raise the mouse-down event and are consumed.
            if !action::MOUSE_ACTIONS.contains(&a) {
                return false;
            }
            self.stats.mouse_downs += 1;
            self.dispatch_mouse(a, true, x, y, dispatch);
            self.ui.mouse_down(a, x, y);
            if dispatch.is_none() {
                self.record_mouse(a, true, x, y);
            }
            return true;
        }
        // Actions 5..0xC release through the mouse up. 0x0D-0x0F are the taps: consumed with no call, so a
        // tap raises 0x40 on the press and nothing at all on the release.
        if action::TAPS.contains(&a) {
            return true;
        }
        if !action::MOUSE_UP_ACTIONS.contains(&a) {
            return false;
        }
        self.stats.mouse_ups += 1;
        self.dispatch_mouse(a, false, x, y, dispatch);
        // The double-click discriminator is the action id (10/11/12); the element's mouse-up
        // handler folds it itself, so nothing is passed here.
        self.ui.mouse_up(a, x, y, false);
        if dispatch.is_none() {
            self.record_mouse(a, false, x, y);
        }
        true
    }

    fn dispatch_mouse(
        &mut self,
        action: u32,
        start: bool,
        x: i32,
        y: i32,
        dispatch: &mut Option<&mut DispatchHook<'_>>,
    ) {
        if let Some(hook) = dispatch.as_deref_mut() {
            let over = self
                .ui
                .mouse_over()
                .and_then(|h| self.ui.node(h))
                .map(dereth_ui::ElementNode::element_id);
            hook(
                self,
                UiDispatch::Mouse(UiMouseEvent {
                    action,
                    start,
                    x,
                    y,
                    over,
                }),
            );
        }
    }

    /// Keep the event and the element under the pointer, for the smart box's own arm.
    fn record_mouse(&mut self, action: u32, start: bool, x: i32, y: i32) {
        let over = self
            .ui
            .mouse_over()
            .and_then(|h| self.ui.node(h))
            .map(dereth_ui::ElementNode::element_id);
        self.mouse_events.push(UiMouseEvent {
            action,
            start,
            x,
            y,
            over,
        });
    }

    // -----------------------------------------------------------------------------------------
    // The screen-layout file's two ends
    // -----------------------------------------------------------------------------------------

    /// The load and save paths share the screen-layout path builder as their prologue.
    ///
    /// The directory is the settings directory — the same
    /// rule used by [`crate::input::keymap_path_for`]. `None` when this build
    /// has no preferences file, which is every test that does not ask for one; the client always
    /// has one because startup sets the default preferences path.
    ///
    /// The height and width are the gameplay screen's **current** size, read off the live root rather
    /// than off the presentation, because that is what the client reads and because a forced
    /// resolution would otherwise look like the same file under two names.
    #[must_use]
    pub fn screen_layout_path(
        &self,
        name: &str,
        preferences_file: &std::path::Path,
        character: &str,
        world: &str,
    ) -> Option<std::path::PathBuf> {
        if preferences_file.as_os_str().is_empty() {
            return None;
        }
        let dir = preferences_file.parent()?;
        let root = self.flow.current()?.roots().first().copied()?;
        let b = self.ui.screen_box(root);
        let file = crate::persist::layout_path("", name, character, world, b.height(), b.width());
        Some(dir.join(file))
    }

    /// Loads a screen layout and reports whether its file opened.
    ///
    /// Returns the value the client stores as its layout-from-file flag: whether the file
    /// opened at all. A first run has no file and that is not an error.
    ///
    /// # Errors
    /// [`dereth_ui::UiError::Persist`] when the file opens and will not parse.
    pub fn load_ui_layout(&mut self, path: &std::path::Path) -> Result<bool, dereth_ui::UiError> {
        let Some(layout) = crate::persist::load_layout_file(path)? else {
            return Ok(false);
        };
        // Split borrow: `flow` holds the screen and `ui` holds the tree it places into, and
        // the placement needs both at once.
        let Some(screen) = crate::hud_drive::game_screen(&mut self.flow) else {
            return Ok(false);
        };
        let dereth_ui_screens::screens::gameplay_host::GameCall::LoadLayout { placed, .. } =
            crate::hud_drive::game_call(
                &mut self.ui,
                screen,
                dereth_ui_screens::screens::gameplay_host::GameCall::LoadLayout {
                    layout,
                    placed: 0,
                },
            )
        else {
            return Ok(false);
        };
        self.stats.screen_layout_windows_placed += placed as u64;
        self.stats.screen_layout_loads += 1;
        Ok(true)
    }

    /// Saves the current screen layout.
    ///
    /// The only client producers are console commands: `@saveui [name]`, which refuses a
    /// 16-character name because its stored length includes the NUL, despite the line *"The file name must
    /// be 16 characters or less."*, and
    /// `@saveautoui`, which sends the `#auto` literal. There is **no
    /// automatic save anywhere in retail** — session ending sets the ending-session flag, checks
    /// the confirmation and logs off, and
    /// destruction of the original gameplay screen ends the session with the false flag and does
    /// nothing else. So "a moved panel is
    /// lost on exit" is **retail's behaviour too**, and adding a save here would be a deviation.
    /// The four commands in `dereth_client_model::cmd::table::INITIALIZE_COMMANDS` raise the matching
    /// host notice in `Interaction::chat_command`; `App` resolves the live path and calls here.
    ///
    /// # Errors
    /// [`dereth_ui::UiError::Persist`] when the file cannot be written.
    pub fn save_ui_layout(&mut self, path: &std::path::Path) -> Result<bool, dereth_ui::UiError> {
        let Some(screen) = crate::hud_drive::game_screen(&mut self.flow) else {
            return Ok(false);
        };
        let dereth_ui_screens::screens::gameplay_host::GameCall::SaveLayout(Some(layout)) =
            crate::hud_drive::game_call(
                &mut self.ui,
                screen,
                dereth_ui_screens::screens::gameplay_host::GameCall::SaveLayout(None),
            )
        else {
            return Ok(false);
        };
        if layout.windows.is_empty() {
            return Ok(false);
        }
        crate::persist::save_layout_file(&layout, path)?;
        self.stats.screen_layout_saves += 1;
        Ok(true)
    }

    /// On player-description arrival, load the automatic screen layout and store whether
    /// it came from a file. Other screens own the remaining arrival effects.
    ///
    /// `None` means the gameplay screen is not up yet, which is not a failure: the notice reaches
    /// whichever `GamePlayScreen` is registered, and in this build the mode switch is queued, so the
    /// caller retries on the next frame. `Some(false)` is "there is no file yet", which is a first
    /// run and is exactly what a false layout-from-file flag records.
    ///
    /// # Errors
    /// [`dereth_ui::UiError::Persist`] when a file exists and will not parse.
    pub fn auto_load_screen_layout(
        &mut self,
        preferences_file: &std::path::Path,
        character: &str,
        world: &str,
    ) -> Option<Result<bool, dereth_ui::UiError>> {
        self.flow.current().filter(|s| s.is_game())?;
        let name = dereth_client_contract::persist::ScreenLayout::AUTO_NAME;
        let path = self.screen_layout_path(name, preferences_file, character, world)?;
        Some(self.load_ui_layout(&path))
    }

    /// The pointer events this frame dispatched, taken once.
    pub fn take_mouse_events(&mut self) -> Vec<UiMouseEvent> {
        std::mem::take(&mut self.mouse_events)
    }

    pub(crate) fn set_target_mode_active(&mut self, active: bool) {
        self.target_mode_active = active;
    }

    // -----------------------------------------------------------------------------------------
    // The movie step
    // -----------------------------------------------------------------------------------------

    /// The frame the host must upload before it draws this frame's UI, taken once.
    pub fn take_movie_frame(&mut self) -> Option<dereth_primitives::TextureData> {
        self.movie_frame.take()
    }

    /// What the movie's soundtrack should do, taken once.
    ///
    /// This crate owns the movie player but not the sound device, so — exactly as the video
    /// frame above is handed up for the host to upload — the audio is handed up for the host to
    /// give to [`crate::audio::Audio`].
    pub fn take_movie_audio_cue(&mut self) -> Option<MovieAudioCue> {
        self.movie_audio_cue.take()
    }

    /// Whether a movie media step is playing right now.
    #[must_use]
    pub fn movie_playing(&self) -> bool {
        self.movie.is_some()
    }

    /// The sound media steps this frame's media machines ran, taken once.
    ///
    /// Each is a [`UiRequest::PlaySound`] carrying the media file name and sound type unchanged;
    /// `crate::audio::apply_sound_requests` is the far end and makes the same choice between
    /// entries 7 and 8 that the media machine makes.
    ///
    /// `dereth_ui` raises [`dereth_ui::MediaEffect::PlaySound`] for every click-sound media step,
    /// and [`Self::tick_media`] — the only reader of `media_effects` — collects them here rather
    /// than dropping them; otherwise no button in the game would make a sound.
    pub fn take_sound_requests(&mut self) -> Vec<UiRequest> {
        std::mem::take(&mut self.sound_requests)
    }

    /// The epilogue screen's character-logoff request, taken once.
    pub fn take_log_off(&mut self) -> bool {
        std::mem::take(&mut self.log_off_requested)
    }

    /// The Key Bindings page's keymap save to its keymap file, taken once.
    pub fn take_save_keymap(&mut self) -> bool {
        std::mem::take(&mut self.save_keymap_requested)
    }

    /// `MediaPlayback`'s two host-side steps: the movie and the sound.
    ///
    /// The DirectShow graph and texture-renderer implementation are replaced. What is preserved is the
    /// contract the rest of the client depends on: **the element reports finished, and posts UI
    /// message `0x1000000D`** — which the media machine does for us, because
    /// the media machine's movie-finished result is what its movie step returns and the
    /// state's own message media step follows it in the list.
    ///
    /// It is not only the movies: it is the sole reader of [`dereth_ui::UiSystem::media_effects`],
    /// and the two effect kinds the media machine hands to a subsystem it does not own are
    /// `PlayMovie` and `PlaySound`. It acts on both.
    fn tick_media(&mut self, now: LocalTime) {
        // 1. New `PlayMovie` and `PlaySound` requests. `media_effects` is taken rather than read,
        //    so an effect is acted on exactly once however many times this runs in a frame.
        let effects = std::mem::take(&mut self.ui.media_effects);
        for (h, e) in effects {
            // The descriptor's two fields cross the seam
            // unchanged: the switch on the sound type belongs to the receiver, which is the track that
            // owns sound playback.
            if let dereth_ui::MediaEffect::PlaySound { file, sound_type } = e {
                self.stats.media_sounds += 1;
                self.sound_requests
                    .push(UiRequest::PlaySound { file, sound_type });
                continue;
            }
            let dereth_ui::MediaEffect::PlayMovie { file_name, .. } = e else {
                continue;
            };
            self.movie_stats.started += 1;
            // The client widens the name and `AddSourceFilter` resolves it against the working
            // directory; a no-database-file name is a plain path, so this is the install directory.
            let path = self.client_dir.join(&file_name);
            match (self.movie_bytes)(&path).and_then(dereth_audio::video::Movie::from_bytes) {
                Some(movie) => {
                    let info = movie.info();
                    let duration = f64::from(info.total_frames)
                        * f64::from(info.micro_sec_per_frame)
                        / 1_000_000.0;
                    // The element draws the surface the renderer blits into. `UiDrawCmd` carries a
                    // `DataID`, so the surface is named by one; see [`MOVIE_IMAGE_ID`].
                    let (w, h_px) = (
                        i32::try_from(info.width).unwrap_or(0),
                        i32::try_from(info.height).unwrap_or(0),
                    );
                    if let Some(n) = self.ui.node_mut(h) {
                        n.region.image = Some(dereth_ui::GraphicRef::opaque_surface(
                            MOVIE_IMAGE_ID,
                            w,
                            h_px,
                        ));
                    }
                    // The soundtrack, if the file has one. Opening the movie builds the whole
                    // graph -- video renderer and audio renderer both -- so the track starts with
                    // the first frame and not on a later tick.
                    self.movie_audio_cue = movie
                        .audio()
                        .map(|a| MovieAudioCue::Start(Box::new(a)))
                        .or(Some(MovieAudioCue::Stop));
                    self.movie = Some(MoviePlayer {
                        element: h,
                        movie,
                        started: now,
                        duration,
                    });
                }
                None => {
                    // A missing or unplayable AVI silently skips that intro state — no error, no
                    // log line. The media step reports finished on the very first tick.
                    self.movie_stats.skipped += 1;
                    if let Some(n) = self.ui.node_mut(h) {
                        n.media.movie_finished = true;
                    }
                }
            }
        }

        // 2. The running movie.
        let Some(p) = self.movie.as_mut() else { return };
        let elapsed = LocalTime(now.0 - p.started.0);
        let info = p.movie.info();
        if let Some(pixels) = p.movie.next_frame(elapsed) {
            self.movie_frame = Some(dereth_primitives::TextureData {
                width: info.width,
                height: info.height,
                // The decoder already writes B, G, R, 0xFF per pixel, top-down: the flip and the
                // forced-opaque alpha are the sample renderer's and live in `dereth-audio`'s media code.
                format: dereth_primitives::TextureFormat::Bgra8,
                levels: vec![pixels.to_vec()],
            });
            self.movie_stats.frames += 1;
        }
        // The finished-playing check returns true on `EC_COMPLETE`, which the graph raises
        // when the last sample has been *presented* — so the last frame stays up for its own
        // 33.367 ms rather than being replaced the instant it is decoded.
        if p.movie.is_finished() && elapsed.0 >= p.duration {
            let element = p.element;
            self.movie = None;
            // The graph is torn down with the last frame, and the audio renderer with it.
            self.movie_audio_cue = Some(MovieAudioCue::Stop);
            self.movie_stats.completed += 1;
            if let Some(n) = self.ui.node_mut(element) {
                n.media.movie_finished = true;
                n.region.image = None;
            }
        }
    }

    /// Character-management requests for the host player-session state, oldest first.
    ///
    /// The character-management screen calls log-on, delete, and restore methods on the player
    /// system from inside its element-message listener. There is no singleton here and
    /// `UiRequest` has no member for any of the three, so the screen records them and this is where
    /// the host collects them. See `dereth_ui_screens::screens::charmgmt::CharacterAction`.
    pub fn take_character_actions(&mut self) -> Vec<CharacterAction> {
        std::mem::take(&mut self.character_actions)
    }

    /// What `CharGenScreen` asked for: `0xF656`, and the log-on that follows a successful
    /// creation.
    pub fn take_chargen_actions(&mut self) -> Vec<CharGenAction> {
        std::mem::take(&mut self.chargen_actions)
    }

    /// Drain `dereth_ui_screens::requests`, acting on the ones this shell owns.
    pub(crate) fn drain_requests(&mut self) -> Vec<UiRequest> {
        let requests = self.ui.requests.take();
        requests
            .into_iter()
            .filter_map(|r| self.handle_request(r))
            .collect()
    }

    /// The visible half of changing the UI-lock flag: after Interaction has updated the retained
    /// module and emitted any `0x0005`, broadcast global `0x0D` so every floaty window re-reads the
    /// chosen value. Kept separate from [`Self::handle_request`] to preserve native's
    /// write-before-broadcast order across the game/UI ownership boundary.
    pub(crate) fn apply_lock_ui(&mut self, locked: bool) {
        self.stats.requests_handled += 1;
        if let Some(g) = crate::hud_drive::game_screen(&mut self.flow) {
            crate::hud_drive::game_call(
                &mut self.ui,
                g,
                dereth_ui_screens::screens::gameplay_host::GameCall::LockUi(locked),
            );
        }
        self.ui
            .broadcast_global(dereth_ui::msg::global::UI_LOCK_TOGGLED, 0);
    }

    /// One shell-owned call, in sequence with the host's owners, not an extraction which can
    /// overtake an earlier item request in the same delivery.
    pub(crate) fn handle_request(&mut self, r: UiRequest) -> Option<UiRequest> {
        match r {
            UiRequest::QueueMode(m) => {
                self.stats.requests_handled += 1;
                self.queue(m);
            }
            UiRequest::DeviceDone => {
                self.stats.requests_handled += 1;
                self.stats.device_done = true;
            }
            // The character screens' player-session operations, queued for the host in the
            // order the screens asked for them. Counted as character actions, as the screen
            // pull they replace was.
            UiRequest::CharacterAction(a) => {
                self.stats.character_actions += 1;
                self.character_actions.push(a);
            }
            UiRequest::CharGenAction(a) => {
                self.stats.chargen_actions += 1;
                self.chargen_actions.push(a);
            }
            // **The writing half of the selection round trip.** The character
            // screen's selected id, mirrored into the persistent selected avatar before the
            // mode switch: mirrored at the top of the frame, the player's very last selection
            // would be lost on the way into the world.
            UiRequest::SelectedAvatar(id) => {
                self.flow.data.selected_avatar = id;
            }
            // The character-generation slot, which the character screen's selection and a
            // refused creation write and the creation request reads.
            UiRequest::CharGenSlot(slot) => {
                self.flow.data.chargen_slot = slot;
            }
            // The epilogue constructor logs off the character when player-session state exists
            // and the network is still up. The screen has no session; the host does.
            UiRequest::EndCharacterSession { .. } => {
                self.stats.requests_handled += 1;
                self.log_off_requested = true;
            }
            // The key-bindings page's *OK* arm saves the current keymap when validation succeeds.
            // The keymap file belongs to the input manager, which is the host's `InputManager`; the
            // page has neither, exactly as it has no player-session state for the line above.
            UiRequest::SaveKeyMap => {
                self.stats.requests_handled += 1;
                self.save_keymap_requested = true;
            }
            // The radar padlock's host handoff.
            //
            // The radar padlock's element-message arm toggles the player's lock flag and then
            // broadcasts global message 0x0D so the visible controls refresh.
            //
            // The flag is the **player's**, not the screen's, which is why the toggle cannot
            // live in `dereth-ui-screens`: the screen emitted `SetLockUi(!locked)` and
            // Interaction owns the flag. Return the requested value untouched. Interaction
            // first runs the UI-lock setter / the 0x33 change notice and queues the visible global-0D half
            // back to [`Self::apply_lock_ui`], preserving native's write-before-broadcast
            // order and its one immediate `0x0005` on an actual change.
            UiRequest::SetLockUi(locked) => {
                return Some(UiRequest::SetLockUi(locked));
            }
            // Send the start-tell notice for `name`; the notice
            // dispatcher walks its listener list and calls the start-tell
            // handler on each; the one override is the chat-entry handler,
            // which lives on the gameplay screen. The friends panel is the
            other => {
                // Not dropped: handed to the caller and counted, so a request nobody owns yet
                // is a number in the report rather than a silence.
                self.stats.requests_ignored += 1;
                return Some(other);
            }
        }
        None
    }

    /// Bring this interface up to the game after frames in which another interface was shown and
    /// this one was not framed: the game went on without it, so the edges it would have seen are
    /// taken as the state they left rather than replayed.
    ///
    /// - The character set the other interface received is this flow's too.
    /// - In the world, the flow goes to the gameplay screen; at character selection, to the
    ///   character screen; disconnected, to the disconnected screen. Anything earlier (still
    ///   connecting) is left to the flow's own steps.
    /// - The host state is taken as seen, so no edge fires on the next frame: a character list
    ///   that arrived meanwhile does not send a player in the world back to character select, and
    ///   an entry into the world that happened meanwhile is not waited for.
    pub fn catch_up(&mut self, host: &HostState) {
        if let Some(set) = host.character_set.as_ref() {
            if host.character_set_notices != self.last_host.character_set_notices
                || host.character_set != self.last_host.character_set
            {
                self.flow.data.on_character_set(set.clone());
            }
        }
        let current = self.flow.current_mode();
        if let Some(text) = host.error.as_ref() {
            if current != Some(mode::DISCONNECTED) {
                self.flow.queue_with_error(mode::DISCONNECTED, text.clone());
            }
        } else if host.in_world {
            if current != Some(mode::GAME_PLAY) {
                self.queue(mode::GAME_PLAY);
            }
        } else if host.character_set.is_some()
            && current != Some(mode::CHARACTER_MANAGEMENT)
            && current != Some(mode::CHAR_GEN)
        {
            self.queue(mode::CHARACTER_MANAGEMENT);
        }
        self.last_host = host.clone();
    }

    /// The two flow-level notice routes and the character set, which reach the flow from outside
    /// the UI entirely.
    fn apply_host_notices(&mut self, host: &HostState) {
        // The persistent-data character-set notice — applied on the edge, because the
        // client's is a notice and a notice arrives once.
        //
        // **The edge is the notice count, not the set's value.** The server re-sends the same
        // character list six seconds after a log-off, and a value comparison cannot see it; see
        // [`HostState::character_set_notices`].
        if let Some(set) = host.character_set.as_ref() {
            // Either signal is a notice: the count is the reliable one and the value comparison is
            // kept because a caller that builds a [`HostState`] by hand (every screen test in this
            // crate) sets the set and not the count. Retail applies it on **every** notice, so
            // firing on either is strictly closer than firing on the value alone.
            if host.character_set_notices != self.last_host.character_set_notices
                || host.character_set != self.last_host.character_set
            {
                self.flow.data.on_character_set(set.clone());
                // The original receiver copies the character set, marks it received and then
                // notifies the active UI flow, in that order. Character management responds by
                // rebuilding the list and closing the *please wait* dialog, which
                // `on_character_set` above already does. The other receiver queues mode
                // `0x1000000A` when character logoff completes.
                //
                // **That is how the retail client returns to character select after a log-off,
                // and it is the only way it does.** The logoff request does *not*
                // queue a mode on the non-quit branch — it logs off the character with the quit
                // flag false and leaves the player standing in the world. The mode
                // changes when the server answers, which in all five recorded sessions is exactly
                // 6.00 s later and is the window the log-off emote, the particle script and the
                // world fade play in.
                if self.flow.current_mode() == Some(mode::GAME_PLAY) {
                    self.queue(mode::CHARACTER_MANAGEMENT);
                }
            }
        }

        // The server-died and character-error notices queue `0x10000002` with the text, **unless
        // the current mode is already `0x10000002`** — "so a second disconnect while on the
        // disconnected screen is ignored".
        if let Some(text) = host.error.as_ref() {
            if host.error != self.last_host.error
                && self.flow.current_mode() != Some(mode::DISCONNECTED)
            {
                self.flow.queue_with_error(mode::DISCONNECTED, text.clone());
            }
        }

        // The begin-enter-world notice is the CharSel → InGame edge. Here it is represented by
        // the session reaching `Playable`.
        //
        // **The wizard is the other framework that answers this notice.** The begin-enter-world
        // notice
        // walks *every* registered non-engine notice receiver and invokes the corresponding handler;
        // **two** receivers handle it — character management **and the creation wizard** — with the
        // same folded function in both, while every other framework leaves the handler as a stub.
        // So in retail a log-on begun
        // from the wizard queues `0x10000008` exactly as one begun from character select does,
        // which is what the creation wizard's log-on relies on. A predicate that named only
        // character select would leave the summary page on screen after `0xF656` → `0xF643` →
        // log-on → `0xF7DF` → `Playable`, with the landblock loaded behind it.
        let from = self.flow.current_mode();
        if host.in_world
            && !self.last_host.in_world
            && (from == Some(mode::CHARACTER_MANAGEMENT) || from == Some(mode::CHAR_GEN))
        {
            self.queue(mode::GAME_PLAY);
        }
    }

    /// The host's pre-game state, handed to the current screen through
    /// [`dereth_ui::framework::Screen::on_pregame`] — the globals a pre-game screen reads.
    ///
    /// Each screen applies what it reads: the data-patch screen the connection and the DDD
    /// events, character management the world name, the list and the verification notice, the
    /// wizard its tables, seeds and the character-set notice, the disconnected screen and the
    /// gameplay screen's logout prompt their string tables. The shell supplies the two notice
    /// edges it tracks and the persistent data the flow owns.
    ///
    /// Returns a mode to queue for the one screen that decides one here: the creation wizard goes
    /// back to character select when the character set
    /// it was waiting for does not contain the name it created.
    fn push_host_state_into_screen(&mut self, host: &HostState) -> Option<UiMode> {
        let char_set = self.flow.data.char_set.clone();
        let char_set_changed = self.last_char_set.as_ref() != Some(&char_set);
        self.last_char_set = Some(char_set.clone());
        // Either signal is the notice, exactly as `apply_host_notices` treats the character set:
        // the count is the reliable one, and the value comparison is kept because a caller that
        // builds a [`HostState`] by hand (every screen test in this crate) sets the code and not
        // the count.
        let chargen_response_changed = host.chargen_response_notices
            != self.last_host.chargen_response_notices
            || host.chargen_response != self.last_host.chargen_response;
        let tables = self
            .chargen_tables
            .clone()
            .map(|t| -> Rc<dyn std::any::Any> { t });
        let p = dereth_ui::framework::PregameCx {
            view: host,
            received_set: self.flow.data.received_set,
            char_set: &char_set,
            char_set_changed,
            selected_avatar: self.flow.data.selected_avatar,
            chargen_slot: self.flow.data.chargen_slot,
            chargen_response_changed,
            ui_strings: self.patch_strings,
            client_strings: self.client_strings,
            tables,
        };
        let screen = self.flow.current_mut()?;
        screen.on_pregame(&mut dereth_ui::framework::ScreenCx::new(&mut self.ui), &p)
    }

    /// Queues a mode after validating its id, with an unregistered-id counter the client does not
    /// have.
    ///
    /// In the client, if the mode id is not in the table nothing happens and the queued mode
    /// **stays set**, so the request is retried every frame forever. Validating the id at
    /// *queue* time prevents that, and costs a counter rather than a hang.
    pub fn queue(&mut self, m: UiMode) {
        if !mode::REGISTRATION_ORDER.contains(&m) {
            self.stats.unregistered_mode_requests += 1;
            return;
        }
        self.flow.queue(m);
    }

    fn note_switch(&mut self, before: Option<UiMode>) {
        let after = self.flow.current_mode();
        if after == before {
            return;
        }
        self.stats.mode_switches += 1;
        // Destroying the outgoing screen destroys its elements and releases their movie player —
        // `IMediaControl::Stop` and the graph
        // torn down. The element this player was blitting into no longer exists.
        self.movie = None;
        self.movie_frame = None;
        // `IMediaControl::Stop` takes the audio renderer down with the rest of the graph, so a
        // mode switch during the logo silences it rather than leaving it playing over the next
        // screen.
        self.movie_audio_cue = Some(MovieAudioCue::Stop);
        if let Some(m) = after {
            self.transitions.push(m);
            // `use_new_mode` leaves no current screen when `Screen::create` fails, which is
            // the same shape as "the factory returned NULL" and is silent. Count it.
            if self.flow.current().is_none() {
                self.stats.screen_create_failures += 1;
            }
        }
    }

    /// Every mode entered so far, in order. The recording the mode-transition test asserts on.
    #[must_use]
    pub fn transitions(&self) -> &[UiMode] {
        &self.transitions
    }

    /// The 2D blit list for this frame, in order.
    ///
    /// Ending the frame with presentation enabled means "the 2D UI overlay, `EndScene`, `Present`"; this
    /// is the first of the three, and [`crate::ui_draw`] is what turns it into quads.
    pub fn draw_list(&mut self) -> Vec<UiDrawCmd> {
        let mut back = RecordingDrawBackend::default();
        self.ui.draw(&mut back);
        back.calls
    }

    /// The display size changed; update the UI layout.
    ///
    /// The window is not resizable, so this only ever runs at start-up here; it exists because the
    /// element manager's `display` must agree with the back buffer or every screen box is wrong.
    pub fn set_display(&mut self, display: (i32, i32)) {
        if self.ui.display() != display {
            self.ui.refresh_event(display);
        }
    }
}

/// Load `CharGen_CharacterData` and the `SkillTable` through the master `DidMapper`.
///
/// `DidMapper 0x25000000` entry **2** is `UNIQUEDB`; inside it entry `0x0E` is
/// `CharGen_CharacterData` (`0x0E000002` in this dat build) and entry `4` is `Weenie_SkillTable`
/// (`0x0E000004`). The shipped mapper names the char-gen table with enum `0x0E`; the indirection is the point
/// — never hard-code either DataID.
fn load_chargen_tables(
    assets: &dyn dereth_primitives::AssetSource,
    store: &Arc<dereth_dat::RetailDatStore>,
) -> Option<Rc<CharGenTables>> {
    use dereth_assets::Decode;
    use dereth_ui::framework::{DidMapperResolver, LayoutEnum, LayoutEnumResolver as _};

    /// The `UNIQUEDB` group of the master `DidMapper`.
    const UNIQUE_DB_GROUP: u32 = 2;
    /// `CharGen_CharacterData` inside it.
    const CHARGEN_ENUM: u32 = 0x0E;
    /// `Weenie_SkillTable` inside it.
    const SKILL_TABLE_ENUM: u32 = 4;

    let r = DidMapperResolver::load_group(assets, UNIQUE_DB_GROUP).ok()?;
    let cg_id = r.resolve(LayoutEnum(CHARGEN_ENUM))?;
    let skill_id = r.resolve(LayoutEnum(SKILL_TABLE_ENUM))?;
    let cg = dereth_assets::tables::CharGen::decode_payload_in(
        assets.container_era_of(cg_id),
        cg_id,
        &assets.read(cg_id).ok()?,
    )
    .ok()?;
    let skills = dereth_assets::tables::SkillTable::decode_payload_in(
        assets.container_era_of(skill_id),
        skill_id,
        &assets.read(skill_id).ok()?,
    )
    .ok()?;
    // The `ClothingTable` of every gear item the table names, which is what
    // character generation reads to learn a style's palette-template
    // list — the value carried by each of the four generated clothing colours. The client asks
    // one clothing table `(id, 0x19)` at a time; `dereth-ui-screens` may not read the dat, so the
    // host loads the closed set once.
    let mut clothing = std::collections::BTreeMap::new();
    for hg in cg.heritage_groups.values() {
        for sx in hg.sexes.values() {
            for item in sx
                .headgear
                .iter()
                .chain(&sx.shirts)
                .chain(&sx.pants)
                .chain(&sx.footwear)
            {
                let id = item.clothing_table;
                if id.0 == 0 || clothing.contains_key(&id) {
                    continue;
                }
                if let Ok(bytes) = assets.read(id) {
                    if let Ok(t) = dereth_assets::motion::ClothingTable::decode_payload_in(
                        assets.container_era_of(id),
                        id,
                        &bytes,
                    ) {
                        clothing.insert(id, t);
                    }
                }
            }
        }
    }
    // The four `UIASSET` images the colour-spot and gradient-disk generators build
    // their surfaces from, through the same indirection everything else uses.
    // An id that will not resolve is left 0 and the wheel draws nothing for it rather than drawing
    // the wrong picture -- and it is counted, not swallowed.
    let did = |e: u32| {
        let r = crate::assets::enum_did(assets, crate::preview::UIASSET_GROUP, e);
        if r.is_none() {
            tracing::warn!("UIASSET enum {e:#010X} (colour wheel) does not resolve");
        }
        r.unwrap_or(dereth_primitives::DataId(0))
    };
    let color_wheel_art = dereth_ui_screens::screens::chargen::ColorWheelArt {
        bullet: did(0x1000_000D),
        ring: did(0x1000_000E),
        empty: did(0x1000_000F),
        plug: did(0x1000_0010),
    };
    let tables = Rc::new(CharGenTables {
        world: Rc::new(dereth_chargen::CreationTables {
            chargen: cg,
            skills,
            clothing: Rc::new(clothing),
        }),
        color_wheel_art,
        colors: Some(Rc::new(DatColorSource {
            store: Arc::clone(store),
            cache: std::cell::RefCell::new(std::collections::BTreeMap::new()),
        })),
    });
    Some(tables)
}

/// Palette and palette-set lookup over the dat, with a cache — the host half of
/// [`dereth_ui_screens::screens::chargen::CgColorSource`].
///
/// The client asks for a palette every time it repaints the wheel and lets the object
/// cache absorb it; this memoises the *answer* instead, which is one `u32` per `(id, index)` pair
/// and keeps the dat read off the repaint path entirely.
struct DatColorSource {
    store: Arc<dereth_dat::RetailDatStore>,
    cache: std::cell::RefCell<
        std::collections::BTreeMap<(dereth_primitives::DataId, u32), Option<u32>>,
    >,
}

impl std::fmt::Debug for DatColorSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatColorSource")
            .field("cached", &self.cache.borrow().len())
            .finish()
    }
}

impl DatColorSource {
    fn read_palette_color(&self, palette: dereth_primitives::DataId, index: u32) -> Option<u32> {
        use dereth_assets::Decode;
        let bytes = dereth_primitives::AssetSource::read(&*self.store, palette).ok()?;
        let p = dereth_assets::material::Palette::decode_payload(palette, &bytes).ok()?;
        // a straight index into the ARGB table. The retail
        // palette is 2,048 entries and every offset the wheel uses (0xB0, 0xD0, 0x103, 0x520) is
        // inside it.
        p.colors_argb.get(usize::try_from(index).ok()?).copied()
    }
}

impl dereth_ui_screens::screens::chargen::CgColorSource for DatColorSource {
    fn pal_set_palettes(
        &self,
        pal_set: dereth_primitives::DataId,
    ) -> Option<Vec<dereth_primitives::DataId>> {
        use dereth_assets::Decode;
        let bytes = dereth_primitives::AssetSource::read(&*self.store, pal_set).ok()?;
        let set = dereth_assets::material::PaletteSet::decode_payload(pal_set, &bytes).ok()?;
        Some(set.palette_ids)
    }

    fn palette_color(&self, palette: dereth_primitives::DataId, index: u32) -> Option<u32> {
        if let Some(hit) = self.cache.borrow().get(&(palette, index)) {
            return *hit;
        }
        let v = self.read_palette_color(palette, index);
        self.cache.borrow_mut().insert((palette, index), v);
        v
    }

    fn pal_set_color(&self, pal_set: dereth_primitives::DataId, index: u32) -> Option<u32> {
        // The key space is shared with `palette_color`'s; a `PalSet` id (0x0F......) and a
        // `Palette` id (0x04......) can never collide.
        if let Some(hit) = self.cache.borrow().get(&(pal_set, index)) {
            return *hit;
        }
        let v = (|| {
            use dereth_assets::Decode;
            let bytes = dereth_primitives::AssetSource::read(&*self.store, pal_set).ok()?;
            let s = dereth_assets::material::PaletteSet::decode_payload(pal_set, &bytes).ok()?;
            let n = u32::try_from(s.palette_ids.len()).ok()?;
            if n == 0 {
                return None;
            }
            // The selection setter's own loop: sum each channel over the whole set, then
            // divide by `num_pals`. Integer division, per channel, alpha not accumulated.
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            for p in &s.palette_ids {
                let c = self.palette_color(*p, index).unwrap_or(0);
                r += (c >> 16) & 0xFF;
                g += (c >> 8) & 0xFF;
                b += c & 0xFF;
            }
            Some(0xFF00_0000 | ((r / n) << 16) | ((g / n) << 8) | (b / n))
        })();
        self.cache.borrow_mut().insert((pal_set, index), v);
        v
    }
}

/// `dereth_ui_screens::env` wants an `Rc<dyn AssetSource>` and the application holds an `Arc`; this
/// is the one line that joins them. It owns nothing and decodes nothing.
#[derive(Debug)]
struct SharedStore(Arc<dereth_dat::RetailDatStore>);

impl dereth_primitives::AssetSource for SharedStore {
    fn read(
        &self,
        id: dereth_primitives::DataId,
    ) -> Result<Vec<u8>, dereth_primitives::AssetError> {
        self.0.read(id)
    }
    fn exists(&self, id: dereth_primitives::DataId) -> bool {
        self.0.exists(id)
    }
    fn iter_type(
        &self,
        kind: dereth_primitives::DataType,
    ) -> Box<dyn Iterator<Item = dereth_primitives::DataId> + '_> {
        self.0.iter_type(kind)
    }
    fn container_era(&self) -> dereth_primitives::ContainerEra {
        self.0.era()
    }
    fn container_era_of(&self, id: dereth_primitives::DataId) -> dereth_primitives::ContainerEra {
        self.0.era_of(id)
    }
}

/// `0xF658 Login_LoginCharacterSet` → the persistent-data object's character set.
pub use dereth_client_runtime::app::character_set_from_login;

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the eight factory-table ids and the switch rule that an unknown id leaves the next
    // mode set, causing the request to be retried every frame forever.
    #[test]
    fn an_unregistered_mode_is_counted_rather_than_queued_for_ever() {
        // The three ids the build does not register.
        for m in dereth_ui_screens::UNREGISTERED_MODES {
            assert!(!mode::REGISTRATION_ORDER.contains(&m), "{m:?}");
        }
        assert_eq!(mode::REGISTRATION_ORDER.len(), 8);
    }

    // Oracle: `dereth_protocol::login::LoginCharacterSet`'s field list; `num_allowed_characters`
    // starts at 5 and is overwritten by the server.
    #[test]
    fn the_character_set_conversion_is_field_for_field() {
        let msg = dereth_protocol::login::LoginCharacterSet {
            status: 7,
            characters: vec![dereth_protocol::login::CharacterIdentity {
                gid: dereth_primitives::ObjectId(0x5000_0001),
                name: "Frostfell".into(),
                seconds_greyed_out: 0,
            }],
            deleted: vec![dereth_protocol::login::CharacterIdentity {
                gid: dereth_primitives::ObjectId(0x5000_0002),
                name: "Doomed".into(),
                seconds_greyed_out: 3600,
            }],
            num_allowed_characters: 11,
            account: "ac01".into(),
            use_turbine_chat: 1,
            has_throne_of_destiny: 1,
        };
        let got = character_set_from_login(&msg);
        assert_eq!(got.set.len(), 1);
        assert_eq!(got.set[0].name, "Frostfell");
        assert_eq!(got.set[0].id, dereth_primitives::ObjectId(0x5000_0001));
        assert_eq!(got.del_set[0].seconds_grace_period, 3600);
        assert_eq!(got.num_allowed_characters, 11);
        assert_eq!(got.account, "ac01");
        assert_eq!(got.status, 7);
        assert!(got.is_throne_of_destiny);

        // A negative slot count cannot be represented and must not wrap to four billion.
        let neg = dereth_protocol::login::LoginCharacterSet {
            num_allowed_characters: -1,
            ..msg
        };
        assert_eq!(character_set_from_login(&neg).num_allowed_characters, 0);
    }

    /// Behaviour: none (both interface projections share their shell's loaded world rules).
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn classic_creation_reuses_its_shell_rules_without_borrowing_another_shells_projection() {
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let first = UiShell::new(&store, (800, 600)).expect("first shell");
        let projection = first.classic_creation_data().expect("creation projection");
        assert!(Rc::ptr_eq(
            &projection.tables,
            &first.chargen_tables.as_ref().expect("world rules").world,
        ));
        let second = UiShell::new(&store, (1024, 768)).expect("second shell");
        let second_projection = second.classic_creation_data().expect("second projection");
        assert!(!Rc::ptr_eq(&projection, &second_projection));
        assert!(Rc::ptr_eq(
            &second_projection.tables,
            &second
                .chargen_tables
                .as_ref()
                .expect("second world rules")
                .world,
        ));
        assert!(Rc::ptr_eq(
            &projection,
            &first.classic_creation_data().unwrap()
        ));
        drop(second);
        assert!(Rc::ptr_eq(
            &projection,
            &first.classic_creation_data().unwrap()
        ));
    }
}
