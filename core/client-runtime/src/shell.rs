//! The front end that plugs into the frame: `Shell`, and `NullShell`, the front end with no UI.
//!
//! [`crate::app::App`] decides the frame's order. A front end is called at each point in that
//! order where the retail client's UI, cursor, clipboard or overlay takes part, and is handed
//! what the step needs: the whole application for the steps that read across it, and just the
//! pieces for the notices delivered while the frame holds the rest. Every call has the answer a
//! client with no UI gives as its default, so `NullShell` is those defaults and nothing else.
//!
//! What a front end may not do is decide sequence: it has no frame of its own, and it reaches the
//! session, the object tables and the world only through the application it is handed.

use std::ops::DerefMut;
use std::path::Path;

use dereth_client_contract::panels::external_container::ExternalContainerNotice;
use dereth_client_contract::panels::salvage::SalvageNotice;
use dereth_client_contract::requests::Outbox;
use dereth_client_contract::view::MagicNotice;
use dereth_client_model::chat::{ChatState, TalkFocusNotice};
use dereth_client_model::combat::PowerBarNotice;
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{LocalTime, ObjectId, Viewport};

use crate::actions::ActionQueue;
use crate::app::{App, StartupError};
use crate::hud::{Hud, HudPanels, HudView, NoPanels};
use crate::interaction::{Interaction, UiLayoutCommand};
use crate::platform::clock::Clock;
use crate::present::{PresentError, Presentation};

/// The notices the UI step drains every frame whether or not there is a UI, handed to the front
/// end's own step ([`Shell::ui_frame`]) to deliver. A notice has no replay history, so one that
/// is not delivered this frame is gone.
#[derive(Debug, Default)]
pub struct UiNotices {
    /// The combat power bar's notices.
    pub power_bar: Vec<PowerBarNotice>,
    /// The external-container panel's notices.
    pub external_container: Vec<ExternalContainerNotice>,
    /// Slumlords the player walked out of range of.
    pub slumlord_range_exits: Vec<ObjectId>,
    /// Books the player walked out of range of.
    pub book_range_exits: Vec<ObjectId>,
    /// The salvage window's notices.
    pub salvage: Vec<SalvageNotice>,
    /// Items accepted onto a secure-trade table.
    pub trade_for_dummies: Vec<ObjectId>,
}

/// Game state a front end's device input routes by, told to it at the moment it changes.
///
/// Which controls reach which actions depends on the game: the attack keys are live only in their
/// combat mode, the target cursor takes the left button while a target mode is up, and the
/// alternate camera controls exist only while their action is held. The runtime owns those
/// states and says when they change; what a front end does about it -- for the retail client, which
/// input maps are registered -- is its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlNotice {
    /// The combat mode's raw value (`1` peace, `2` melee, `4` missile, `8` magic).
    CombatMode(u32),
    /// Whether a use or examine target mode is up.
    TargetMode(bool),
    /// Whether the camera's alternate-mode action is held.
    AlternateCamera(bool),
}

/// A front end: the UI and everything else that draws over the world or answers the player.
///
/// The associated types are the front end's own: the panels the HUD model feeds as events land,
/// the HUD it holds (the model, and whatever it adds), and the presentation it draws through.
#[allow(unused_variables)]
pub trait Shell: Sized {
    /// The panels the HUD model offers lines and failures to as events land.
    type Panels: HudPanels;
    /// The HUD the application holds: the model over [`Self::Panels`], and whatever the front end
    /// adds beside it.
    type Hud: DerefMut<Target = Hud<Self::Panels>> + Default + std::fmt::Debug;
    /// The presentation: the device, and whatever overlay the front end draws through it.
    type Present: ?Sized + Presentation;

    // ---------------------------------------------------------------------------------------
    // start-up
    // ---------------------------------------------------------------------------------------

    /// Bring the front end's device input up: the first of the start-up shell's three steps, before
    /// sound and the UI, because a text element registers its controls as soon as a screen builds
    /// one. A front end with no devices has nothing to start.
    fn start_input(&mut self, app: &mut App<Self>) {}

    /// Bring the UI up, after input and sound. Asked only when the configuration wants a UI.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the UI's own data is missing.
    fn start_ui(&mut self, app: &mut App<Self>) -> Result<(), StartupError> {
        Ok(())
    }

    // ---------------------------------------------------------------------------------------
    // what the frame asks
    // ---------------------------------------------------------------------------------------

    /// Whether there is a UI at all.
    fn has_ui(&self) -> bool {
        false
    }
    /// Whether the current UI mode is the gameplay screen: the player is in the world.
    fn in_gameplay(&self) -> bool {
        false
    }
    /// Whether the current screen hides the world (the credits, which draw on a cleared frame).
    fn hides_world(&self) -> bool {
        false
    }
    /// The generation of the chat subscribers a synchronous chat notice reaches: `Some` while the
    /// gameplay screen is up, changing each time it is rebuilt.
    fn chat_generation(&self) -> Option<u64> {
        None
    }
    /// The UI's request queue, which the HUD model writes the requests it raises into.
    fn ui_requests(&mut self) -> Option<&mut Outbox> {
        None
    }
    /// The 3D viewport's rectangle; `None` is the whole back buffer.
    fn game_viewport(&self) -> Option<Viewport> {
        None
    }
    /// Whether the pointer at `cursor` is over the 3D view rather than over a window drawn on top
    /// of it.
    fn pointer_over_game_view(&self, cursor: (i32, i32)) -> bool {
        true
    }
    /// Whether the examination panel is on screen.
    fn examine_panel_open(&mut self) -> bool {
        false
    }
    /// The server's disconnect notice for `event`, as the disconnected screen shows it: a string
    /// id, or the text itself for the two notices that carry their own sentence. `None` for an
    /// event that raises none.
    fn disconnect_message(&self, event: &SessionEvent, clock: &dyn Clock) -> Option<String> {
        None
    }

    // ---------------------------------------------------------------------------------------
    // the UI step
    // ---------------------------------------------------------------------------------------

    /// Show, answer and close the callback dialogs the interaction layer asked for. With no UI to
    /// show them in, the requests are dropped: a question nobody can see is never answered.
    fn service_dialogs(
        &mut self,
        interaction: &mut Interaction,
        world: &mut dereth_client_model::World,
        now: LocalTime,
    ) {
        interaction.discard_unshown_dialogs();
    }
    /// Before the UI takes its input: whether the player is airborne (the log-off confirmation
    /// refuses an airborne log-off), and the chat-focus notices raised since the last delivery.
    fn before_ui_input(
        &mut self,
        chat: &mut ChatState,
        player_airborne: bool,
        notices: Vec<TalkFocusNotice>,
    ) {
    }
    /// The device input a window drain gathered, in arrival order: called once per frame from the
    /// event-loop step, right after the window's own lifecycle events. A front end whose window
    /// keeps its events for itself handles all of them here -- the lifecycle ones through
    /// [`App::handle_window_event`], the device ones through its own input and on into actions.
    fn window_input(&mut self, app: &mut App<Self>, time_ms: u32) {}
    /// The device input's per-frame tick when there is no UI to give it one: the repeat sweep,
    /// which is what makes a held control repeat at all. With a UI, the UI's own update ticks it.
    fn input_use_time(&mut self, app: &mut App<Self>, now: LocalTime) {}
    /// Hand this frame's actions on: whatever the device input produced and the UI did not
    /// consume, for the frame's other handler stages. Called at the foot of the UI step.
    fn hand_on_actions(&mut self, actions: &mut ActionQueue) {}
    /// Something the device input's routing depends on changed. See [`ControlNotice`].
    fn control_notice(&mut self, notice: ControlNotice) {}
    /// Press the pre-game screens' own buttons for a scripted world entry.
    fn drive_pregame_screens(&mut self, app: &mut App<Self>) {}
    /// The UI's own step: the screen update, the mode switch, the HUD and the panels, the preview
    /// spaces, and the requests the screens raised.
    fn ui_frame(&mut self, app: &mut App<Self>, now: LocalTime, notices: UiNotices) {}

    // ---------------------------------------------------------------------------------------
    // notices delivered as they are raised
    // ---------------------------------------------------------------------------------------

    /// Chat-focus notices, synchronous with the command that raised them.
    fn deliver_chat_focus_notices(&mut self, chat: &mut ChatState, notices: Vec<TalkFocusNotice>) {}
    /// Power-bar notices, synchronous with the jump or control change that raised them.
    fn deliver_power_bar_notices(&mut self, hud: &mut Self::Hud, notices: Vec<PowerBarNotice>) {}
    /// An object notice, offered to the panels that watch objects.
    fn object_panel_notice(
        &mut self,
        hud: &mut Self::Hud,
        interaction: &mut Interaction,
        world: &mut dereth_client_model::World,
        notice: &dereth_client_model::Notice,
    ) {
    }
    /// The magic notices this frame's input raised. Asked only when there is a UI; with none they
    /// stay where they are.
    fn emit_magic_notices(&mut self, notices: Vec<MagicNotice>) {}

    // ---------------------------------------------------------------------------------------
    // the host effects of the interaction step
    // ---------------------------------------------------------------------------------------

    /// Open the vendor panel's buying tab.
    fn open_vendor_buying(&mut self, hud: &mut Self::Hud) {}
    /// The four screen-layout commands, against the layout file for this character and world.
    fn run_ui_layout_commands(
        &mut self,
        preferences_file: &Path,
        character: &str,
        world: &str,
        commands: Vec<UiLayoutCommand>,
    ) {
    }
    /// Close the examination panel.
    fn close_examine_panel(&mut self) {}
    /// The toolbar's split-stack notice for `selected`.
    fn split_stack(&mut self, view: &HudView<'_, Self::Panels>, selected: ObjectId) {}
    /// Dispatch an input action to the UI's listeners. `Some(answered)` is the dispatch's own
    /// answer; `None` is no UI to dispatch into.
    fn dispatch_input_action(&mut self, action: u32) -> Option<bool> {
        None
    }

    // ---------------------------------------------------------------------------------------
    // the world-view step and the frame's end
    // ---------------------------------------------------------------------------------------

    /// The smart box's tooltip for the object the pick found.
    fn world_tooltip(&mut self, interaction: &mut Interaction) {}
    /// The target indicator over the tracked object.
    fn draw_world_target(&mut self, app: &mut App<Self>) {}
    /// Choose and apply the cursor.
    fn update_cursor(&mut self, app: &mut App<Self>) {}
    /// Move text between the UI and the system clipboard.
    fn sync_clipboard(&mut self) {}
    /// Build this frame's overlay and upload what it newly references, outside the frame bracket.
    fn compose_ui(&mut self, app: &mut App<Self>) {}
    /// Draw the overlay over the finished world, inside `PresentFrame`.
    ///
    /// # Errors
    /// Whatever the device answers.
    fn draw_ui(&mut self, present: &mut Self::Present) -> Result<(), PresentError> {
        Ok(())
    }
    /// The display's new extent, after a presentation change.
    fn set_display(&mut self, display: (i32, i32)) {}

    // ---------------------------------------------------------------------------------------
    // shutdown
    // ---------------------------------------------------------------------------------------

    /// Save the device bindings the player may have changed, for the next run. The shutdown step
    /// records what this answers: a front end with no bindings to save has nothing to do.
    fn save_bindings(&mut self) -> crate::shutdown::Outcome {
        crate::shutdown::Outcome::Nothing
    }
    /// Take the UI down: its roots, their textures, the element manager. Before the data files
    /// close, because UI elements hold dat objects.
    fn cleanup_ui(&mut self, app: &mut App<Self>) {}
}

/// The HUD model with no panels beside it: what a front end with no UI holds.
#[derive(Debug, Default)]
pub struct PlainHud(pub Hud<NoPanels>);

impl std::ops::Deref for PlainHud {
    type Target = Hud<NoPanels>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PlainHud {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// The front end with no UI: a headless run, or a client started without its UI. Every call is
/// the answer a client with nothing on screen gives.
#[derive(Debug, Default)]
pub struct NullShell;

impl Shell for NullShell {
    type Panels = NoPanels;
    type Hud = PlainHud;
    type Present = dyn Presentation;
}
