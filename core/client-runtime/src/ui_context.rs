//! What a front end is given at each step of the frame it takes part in.
//!
//! A front end never holds the application. Each [`Shell`](crate::shell::Shell) hook that does
//! more than draw is handed a [`UiContext`](crate::ui_context::UiContext): the game as a front end
//! may see it (the model read-only, the pre-game state, the scene as a view), the front end's own
//! HUD slot, the presentation, and a named call for each thing a front end may ask the game to do.
//! Every front end gets the same context, the modern UI included, so nothing a UI does goes past
//! it.

use std::sync::Arc;

use dereth_client_contract::UiRequest;
use dereth_client_model::chat::{ChatState, TalkFocus, TalkFocusNotice};
use dereth_client_model::World;
use dereth_primitives::{LocalTime, ObjectId, ServerTime};

use crate::config::Config;
use crate::interaction::{TargetMode, UiMouseEvent};
use crate::objects::ObjectStream;
use crate::present::{Presentation, Scene};
use crate::shell::Shell;
use {
    crate::app::App, crate::app::EnterWorldScript,
    dereth_client_contract::pregame::PregameView as HostState,
};

/// Interface-owned live settings. These never replace the saved shared preferences.
#[derive(Debug, Clone, Copy)]
pub enum InterfaceOverrides {
    /// Classic transforms vertical motion itself, so shared inversion stays off.
    ClassicInput,
    /// The Classic viewport determines its live field of view.
    ClassicViewport(f32),
    /// A successful return to Modern restores the stored input and view choices.
    Modern,
}

/// One step's view of the game for front end `S`. See the module documentation.
pub struct UiContext<'a, S: Shell> {
    app: &'a mut App<S>,
}

impl<S: Shell> std::fmt::Debug for UiContext<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiContext").finish_non_exhaustive()
    }
}

impl<'a, S: Shell> UiContext<'a, S> {
    pub(crate) fn new(app: &'a mut App<S>) -> Self {
        Self { app }
    }

    // ---- what the front end reads

    /// The run's configuration.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.app.cfg
    }

    /// The portal dats.
    #[must_use]
    pub fn store(&self) -> &Arc<dereth_dat::RetailDatStore> {
        &self.app.store
    }

    /// How many times the data files have been reopened since start-up: a front end keeping
    /// anything it read from [`Self::store`] reads it again when this moves.
    #[must_use]
    pub fn store_generation(&self) -> u64 {
        self.app.store_generation()
    }

    /// The animation assets the preview spaces build their objects from.
    #[must_use]
    pub fn anim_assets(&self) -> &Arc<dereth_world_data::anim_assets::DatAnimAssets> {
        &self.app.anim_assets
    }

    /// This frame's time, in seconds.
    #[must_use]
    pub fn now(&self) -> f64 {
        self.app.timer.cur_time
    }

    /// Whether the screen-size choice is still waiting for the host or the player.
    pub fn resolution_pending(&self) -> bool {
        self.app.resolution.pending()
    }
    /// The size to preserve when another settings apply occurs during a test.
    pub fn resolution_previous(&self) -> Option<(u32, u32)> {
        self.app.resolution.previous()
    }
    /// The completed choice, for page draft readback without resetting a fresh selection.
    pub fn resolution_completion(&self) -> Option<crate::resolution::ResolutionCompletion> {
        self.app.resolution.completion()
    }

    /// The pre-game state: the connection, the character list, the world's name.
    #[must_use]
    pub fn pregame(&self) -> &HostState {
        &self.app.host_state
    }

    /// The running program's version: what follows its name in the identity `@version` prints
    /// (`0.2.0` of `dereth-client 0.2.0`).
    #[must_use]
    pub fn client_version(&self) -> &'static str {
        let id = self.app.interaction.client_build_id;
        id.rsplit(' ').next().unwrap_or(id)
    }

    /// The scripted entry (`--enter-world`) the pre-game screens are driving.
    #[must_use]
    pub fn entry_script(&self) -> EnterWorldScript {
        self.app.script
    }

    /// How busy the game is: nonzero while anything the player asked for is still waiting. See
    /// [`App::busy_count`].
    #[must_use]
    pub fn busy_count(&self) -> u32 {
        self.app.busy_count()
    }

    /// The vivid-target indicator's global switch.
    #[must_use]
    pub fn vivid_target_indicator(&self) -> bool {
        self.app.teleport.anim.vivid_target_indicator
    }

    /// The game model, read-only.
    #[must_use]
    pub fn model(&self) -> &World {
        &self.app.objects.world
    }

    /// Set the notebook identity before either interface reads its shared pages.
    pub fn prepare_journal(
        &mut self,
        identity: Option<dereth_client_contract::journal::JournalIdentity>,
    ) {
        self.app.objects.world.journal.set_identity(identity);
    }

    pub fn take_journal_io(&mut self) -> Vec<dereth_client_contract::journal::JournalIo> {
        self.app.objects.world.journal.take_io()
    }

    pub fn record_journal_io(
        &mut self,
        identity: &dereth_client_contract::journal::JournalIdentity,
        generation: u64,
        read: bool,
        success: bool,
    ) {
        self.app
            .objects
            .world
            .journal
            .record_io(identity, generation, read, success);
    }

    pub fn complete_journal_load(
        &mut self,
        identity: dereth_client_contract::journal::JournalIdentity,
        generation: u64,
        revision: u64,
        pages: Result<Vec<dereth_client_contract::journal::JournalPage>, ()>,
    ) -> bool {
        self.app
            .objects
            .world
            .journal
            .complete_load(identity, generation, revision, pages)
    }

    /// The object tables, read-only.
    #[must_use]
    pub fn objects(&self) -> &ObjectStream {
        &self.app.objects
    }

    /// Where `id` stands against the player: whether it is outdoors (on the landscape rather than
    /// in a building or a dungeon) and how far it is, in metres. `None` while either has no place
    /// in the world.
    #[must_use]
    pub fn object_place(&self, id: ObjectId) -> Option<(bool, f32)> {
        let objects = &self.app.objects;
        let me = objects.presence(objects.player()?)?.position.as_ref()?;
        let it = objects.presence(id)?.position.as_ref()?;
        let offset = dereth_physics::math::localtolocal(me, it, dereth_primitives::Vec3::ZERO);
        Some((
            dereth_physics::landdefs::is_outdoors(it.cell),
            offset.magnitude(),
        ))
    }

    /// The drawn world as one read-only view, `None` before there is one.
    #[must_use]
    pub fn scene(&self) -> Option<Box<dyn Scene + '_>> {
        self.app.present.scene(self.app.world.as_ref())
    }

    /// The way the camera looks, in degrees clockwise from north, as the player heading is
    /// given: the orbit camera's as shown while an interface uses it, else the game camera's.
    /// `None` without a player in the world.
    #[must_use]
    pub fn camera_heading(&self) -> Option<f32> {
        let c = self.app.world.as_ref()?.character.as_ref()?;
        Some(if c.camera.orbit_active {
            c.camera.orbit.facing_degrees()
        } else {
            dereth_physics::math::get_heading(&c.camera.viewer.frame)
        })
    }

    /// The interaction layer's target mode.
    #[must_use]
    pub fn target_mode(&self) -> TargetMode {
        self.app.interaction.target_mode()
    }

    /// The object under the pointer, as last found.
    #[must_use]
    pub fn found_object(&self) -> ObjectId {
        self.app.interaction.pick.click_object().0
    }

    /// Whether the interaction layer is looking for an object under the pointer (a pick the UI
    /// asked for is still under way).
    #[must_use]
    pub fn looking_for_object(&self) -> bool {
        self.app.interaction.pick.looking_for_object()
    }

    /// Where the pointer last was over the window, in window pixels.
    #[must_use]
    pub fn last_cursor(&self) -> Option<(f64, f64)> {
        self.app.last_cursor
    }

    /// Whether the mouse is looking around (the pointer turns the camera and stays put).
    #[must_use]
    pub fn mouse_look(&self) -> bool {
        self.app.mouse_look
    }

    /// Whether the game is full screen now.
    #[must_use]
    pub fn full_screen(&self) -> bool {
        self.app.applied_full_screen
    }

    /// Draw the world around `landblock` behind the screens before the player is in it, the
    /// camera turned and tilted by `yaw` and `pitch` radians from where the scene starts it. See
    /// [`App::show_backdrop`]. True while it is up.
    pub fn show_backdrop(&mut self, landblock: u16, yaw: f32, pitch: f32) -> bool {
        self.app.show_backdrop(landblock, yaw, pitch)
    }

    /// Release the backdrop [`Self::show_backdrop`] drew.
    pub fn hide_backdrop(&mut self) {
        self.app.hide_backdrop();
    }

    /// Whether the player is travelling through portal space.
    #[must_use]
    pub fn teleporting(&self) -> bool {
        self.app.teleport.anim.state != dereth_client_contract::teleport::TeleportAnimState::Off
    }

    /// The screen sizes the display offers.
    #[must_use]
    pub fn display_modes(&self) -> Vec<(u32, u32)> {
        use dereth_client_contract::options::store;
        store::display_choices(store::DISPLAY_RESOLUTION)
            .iter()
            .map(|mode| store::mode_size(mode.value))
            .collect()
    }

    // ---- the front end's own

    /// The front end's HUD slot: the HUD model and whatever the front end keeps beside it.
    #[must_use]
    pub fn hud(&self) -> &S::Hud {
        &self.app.hud
    }

    /// …writable.
    pub fn hud_mut(&mut self) -> &mut S::Hud {
        &mut self.app.hud
    }

    /// The HUD slot and the object tables it is drawn from, together.
    pub fn hud_and_objects(&mut self) -> (&mut S::Hud, &ObjectStream) {
        (&mut self.app.hud, &self.app.objects)
    }

    /// The presentation.
    #[must_use]
    pub fn present(&self) -> &S::Present {
        &self.app.present
    }

    /// …writable.
    pub fn present_mut(&mut self) -> &mut S::Present {
        &mut self.app.present
    }

    /// The presentation, with the drawn world's state for the calls that need it.
    pub fn present_with_world(
        &mut self,
    ) -> (&mut S::Present, Option<&crate::world_state::WorldState>) {
        (&mut self.app.present, self.app.world.as_ref())
    }

    /// The sound mixer, when there is one.
    pub fn audio_mut(&mut self) -> Option<&mut crate::audio::Audio> {
        self.app.audio.as_mut()
    }

    /// Switch one talk focus on or off. The switch raises a notice, which
    /// [`Self::deliver_talk_focus_notices`] offers back.
    pub fn set_talk_focus_enabled(&mut self, focus: TalkFocus, enabled: bool) {
        self.app
            .objects
            .world
            .chat
            .set_talk_focus_enabled(focus, enabled);
    }

    /// The talk-focus notices raised since the last delivery, offered now. See
    /// [`offer_talk_focus_notices`].
    pub fn deliver_talk_focus_notices(&mut self, answer: &mut TalkFocusAnswer<'_>) {
        let chat = &mut self.app.objects.world.chat;
        let notices = chat.take_talk_focus_notices();
        offer_talk_focus_notices(chat, notices, answer);
    }

    // ---- what the front end asks the game to do

    /// What the runtime owes one request a UI raised, done at once. See [`App::run_request`]. The
    /// talk-focus notices the request raises are offered to `talk_focus` as they are raised.
    pub fn run_request(
        &mut self,
        request: UiRequest,
        now: LocalTime,
        talk_focus: &mut TalkFocusAnswer<'_>,
    ) -> Vec<UiRequest> {
        self.app.run_request(request, now, &mut |chat| {
            let notices = chat.take_talk_focus_notices();
            offer_talk_focus_notices(chat, notices, talk_focus);
        })
    }

    /// Apply interface-local overrides at the caller's existing transition/frame boundary.
    /// Shared sound settings are deliberately outside this policy.
    pub fn apply_interface_overrides(&mut self, overrides: InterfaceOverrides) {
        use dereth_client_contract::{
            options::{names, store},
            PrefValue,
        };
        if let InterfaceOverrides::ClassicViewport(fov) = overrides {
            let current = self.scene().map(|s| s.render_preferences().field_of_view);
            if current.is_some_and(|c| (c - fov).abs() > f32::EPSILON) {
                self.present_mut().apply_render_preference_requests(vec![
                    UiRequest::SetPreference(names::FIELD_OF_VIEW, PrefValue::Float(fov)),
                ]);
            }
            return;
        }
        let now = LocalTime(self.now());
        match overrides {
            InterfaceOverrides::ClassicInput => {
                self.apply_interface_preference(
                    UiRequest::SetPreference(
                        names::INVERT_MOUSE_LOOK_Y_AXIS,
                        PrefValue::Bool(false),
                    ),
                    now,
                );
            }
            InterfaceOverrides::Modern => {
                for name in [names::INVERT_MOUSE_LOOK_Y_AXIS, names::FIELD_OF_VIEW] {
                    if let Some(value) = store::inq_value(name) {
                        self.apply_interface_preference(UiRequest::SetPreference(name, value), now);
                    }
                }
            }
            InterfaceOverrides::ClassicViewport(_) => unreachable!(),
        }
    }

    fn apply_interface_preference(&mut self, request: UiRequest, now: LocalTime) {
        let remaining = self.run_request(request, now, &mut |_, _| false);
        let remaining = self
            .present_mut()
            .apply_render_preference_requests(remaining);
        crate::camera::apply_preference_requests(
            self.app
                .world
                .as_mut()
                .and_then(|world| world.character.as_mut()),
            remaining,
        );
    }

    /// See [`App::deliver_selection_notices`].
    pub fn deliver_selection_notices(&mut self, now: LocalTime) {
        self.app.deliver_selection_notices(now);
    }

    /// The actions `--action-at` presses this frame, taken: the front end presses each as its
    /// key would be, through its own input.
    pub fn take_scripted_actions(&mut self) -> Vec<dereth_client_contract::actions::ActionId> {
        std::mem::take(&mut self.app.scripted_actions)
    }

    /// Queue an action for the next frame, as a device would produce it: a scripted run presses
    /// the game's keys through this. See [`App::inject_action`].
    pub fn inject_action(&mut self, action: crate::actions::Action) {
        self.app.inject_action(action);
    }

    /// Accept actions immediately, including releases from an outgoing input scope.
    pub fn accept_actions(&mut self, actions: impl IntoIterator<Item = crate::actions::Action>) {
        self.app.actions.submit(actions);
    }

    /// One pass of the dialog service with `presenter` showing the questions, or none to drop
    /// them.
    pub fn service_dialogs(
        &mut self,
        presenter: Option<&mut dyn crate::dialogs::DialogPresenter>,
        now: LocalTime,
    ) {
        self.app.service_dialogs_with(presenter, now);
    }

    /// The UI's sound requests, played at once; anything that is not a sound comes back.
    pub fn play_sounds(&mut self, requests: Vec<UiRequest>) -> Vec<UiRequest> {
        crate::audio::apply_sound_requests(self.app.audio.as_mut(), &self.app.store, requests)
    }

    /// Hand the pointer events and the requests no step took this frame to the interaction step.
    pub fn queue(&mut self, mouse: Vec<UiMouseEvent>, requests: Vec<UiRequest>) {
        self.app.interaction.queue(mouse, requests);
    }

    /// The HUD's own placement owners, applied to `requests` at once; what they took is removed.
    pub fn consume_placement_requests(&mut self, requests: &mut Vec<UiRequest>, now: ServerTime) {
        self.app
            .hud
            .consume_placement_requests(&mut self.app.objects.world, requests, now);
    }

    /// The pointer's place for the next world pick, in window pixels: where an item an
    /// interface dragged was let go over the world.
    pub fn note_pointer(&mut self, x: i32, y: i32) {
        self.app.interaction.note_cursor((x, y));
    }

    /// A pointer event over the world.
    pub fn pointer(&mut self, event: UiMouseEvent) {
        let viewport = self.app.present.size();
        self.app.interaction.dispatch_ui_mouse(event, viewport);
    }

    /// The right button came up, ending a drag when `dragged`: the release that reaches the world
    /// examines nothing then, wherever it lands.
    pub fn note_right_release(&mut self, dragged: bool) {
        self.app.interaction.note_right_release(dragged);
    }

    /// The pointer resting at `position` over the world, over item `item` if a slot is under it.
    pub fn hover(
        &mut self,
        position: (i32, i32),
        item: Option<ObjectId>,
        over_view: bool,
        now: LocalTime,
    ) {
        let viewport = self.app.present.size();
        let inter = &mut self.app.interaction;
        inter.note_pointer_over_game_view(over_view);
        inter.dispatch_ui_hover(
            position,
            item,
            viewport,
            &mut self.app.objects.world,
            ServerTime(now.0),
        );
    }

    /// The selection blink's once-a-loop step.
    pub fn selection_blink(&mut self, now: LocalTime) {
        self.app.interaction.global_loop_lighting(now.0);
    }

    /// The selection lighting raised since the last call, applied to the drawn world.
    pub fn apply_selection_lighting(&mut self) {
        for (id, mode) in self.app.interaction.take_pending_lighting() {
            if let Some(mut scene) = self.app.present.scene_mut(self.app.world.as_mut()) {
                scene.world_mut().apply_object_lighting(id, mode);
            }
        }
    }

    /// The selection lighting raised since the last call, handed to a front end that lights the
    /// world its own way.
    pub fn take_selection_lighting(
        &mut self,
    ) -> Vec<(ObjectId, dereth_animation::parts::LightingMode)> {
        self.app.interaction.take_pending_lighting()
    }

    /// Light object `id` in the drawn world as `mode` says.
    pub fn light_object(&mut self, id: ObjectId, mode: dereth_animation::parts::LightingMode) {
        if let Some(mut scene) = self.app.present.scene_mut(self.app.world.as_mut()) {
            scene.world_mut().apply_object_lighting(id, mode);
        }
    }

    /// The interface that has actually accepted input, after any requested switch succeeds.
    pub fn set_chat_interface(
        &mut self,
        interface: dereth_client_contract::options::interface::Interface,
    ) {
        self.app.interaction.chat_interface = interface;
    }

    /// Ordered text changes raised by shared entry operations.
    pub fn take_chat_entry_updates(
        &mut self,
    ) -> Vec<dereth_client_contract::chat::entry::EntryUpdate> {
        self.app.interaction.take_chat_entry_updates()
    }

    /// The retained drafts, without focus effects or replayed commands.
    pub fn chat_entry_drafts(&self) -> Vec<dereth_client_contract::chat::entry::EntryUpdate> {
        self.model()
            .chat
            .entries
            .iter()
            .map(
                |(&window, entry)| dereth_client_contract::chat::entry::EntryUpdate {
                    window,
                    text: entry.text.clone(),
                    cursor: entry.text.chars().count(),
                    focus: false,
                },
            )
            .collect()
    }

    /// Chat-window titles set by command since the last call.
    pub fn take_chat_window_titles(&mut self) -> Vec<(u32, String)> {
        self.app.interaction.take_chat_window_title_notices()
    }

    /// Frame-rate display switches set by command since the last call.
    pub fn take_framerate_display_switches(&mut self) -> Vec<bool> {
        self.app.interaction.take_framerate_display_notices()
    }

    /// A local line in the text scroll.
    pub fn add_scroll_line(&mut self, text: &str, kind: u32) {
        self.app
            .objects
            .world
            .scroll
            .add_text_to_scroll(text, kind, true, 0);
    }

    /// Insert a line whose producer supplies viewport meaning.
    pub fn add_feedback_line(
        &mut self,
        text: &str,
        kind: u32,
        feedback: dereth_client_contract::feedback::Feedback,
    ) {
        self.app
            .objects
            .world
            .scroll
            .add_feedback_to_scroll(text, kind, true, 0, feedback);
    }

    /// The external-container panel's range watches end (the panel that held them is gone).
    pub fn end_external_container_watches(&mut self) {
        self.app
            .objects
            .world
            .object_range_checks
            .unregister_all(dereth_client_model::range::RangeHandler::ExternalContainer);
    }

    /// Bring the preferences up into the UI's option store. See [`App::start_preferences`].
    pub fn start_preferences(&mut self) {
        self.app.start_preferences();
    }

    /// The HUD model's once-a-frame sync. See [`App::sync_hud`].
    pub fn sync_hud(&mut self) {
        self.app.sync_hud();
    }

    /// Whether the saved screen layout is waiting to be loaded.
    #[must_use]
    pub fn screen_layout_pending(&self) -> bool {
        self.app.pending_auto_layout
    }

    /// The saved screen layout was loaded, or will not be.
    pub fn screen_layout_done(&mut self) {
        self.app.pending_auto_layout = false;
    }

    /// The character screen's asks: log on, delete, restore.
    pub fn run_character_actions(
        &mut self,
        actions: Vec<dereth_client_contract::pregame::CharacterAction>,
    ) {
        self.app.run_character_actions(actions);
    }

    /// The creation wizard's asks.
    pub fn run_chargen_actions(
        &mut self,
        actions: Vec<dereth_client_contract::pregame::CharGenAction>,
    ) {
        self.app.run_chargen_actions(actions);
    }

    /// Log the character off.
    pub fn log_off_character(&mut self) {
        self.app.log_off_character();
    }

    /// Quit: the device is done.
    pub fn quit(&mut self) {
        self.app.done();
    }

    /// Leave the game from a front end's own Exit: from the world the character logs off first,
    /// its player module saved ahead of the departure. See [`App::quit_game`].
    pub fn quit_game(&mut self) {
        self.app.quit_game();
    }

    /// The teleport overlay's step. See [`App::teleport_use_time`].
    pub fn teleport_use_time(&mut self, shell: &S) {
        self.app.teleport_use_time(shell);
    }

    /// The portal space's step. See [`App::portal_space_use_time`].
    pub fn portal_space_use_time(&mut self, shell: &mut S) {
        self.app.portal_space_use_time(shell);
    }

    /// The gameplay screen came or went: follow its forced resolution and full-screen state.
    pub fn follow_screen_change(&mut self, shell: &mut S) {
        self.app.follow_screen_forced_resolution(shell);
        self.app.follow_gameplay_full_screen(shell);
    }

    // ---- the window

    /// A window lifecycle event (resize, focus, close). See [`App::handle_window_event`].
    pub fn window_event(
        &mut self,
        shell: &mut S,
        event: &crate::platform::window::HostEvent,
        time_ms: u32,
    ) {
        self.app.handle_window_event(shell, event, time_ms);
    }

    /// A device message, through the window procedure.
    pub fn window_message(&mut self, message: crate::pump::WindowMessage, time_ms: u32) {
        self.app.pump.dispatch(message, time_ms);
    }

    /// The mouse-look button.
    pub fn mouse_look_button(&mut self, down: bool) {
        self.app.mouse_look_button(down);
    }

    /// Use the orbit camera in place of the game's own, with `settings`; `None` goes back to the
    /// game's camera, which is where it was left. The orbit camera keeps its own place meanwhile.
    pub fn set_orbit_camera(&mut self, settings: Option<crate::orbit::OrbitSettings>) {
        if settings.is_none() {
            self.app.orbit_keys = crate::orbit::MovementKeys::default();
            self.app.orbit_pending.clear();
            self.app.orbit_look = (0.0, 0.0);
        }
        self.app.orbit = settings;
    }

    /// The renderers this build can create, the one drawing, and the ones the preferences file
    /// and the command line named at start-up, for the options page's renderer choice.
    #[must_use]
    pub fn renderer_status(&self) -> dereth_client_contract::options::renderer::RendererStatus {
        dereth_client_contract::options::renderer::RendererStatus {
            preference: self.app.cfg.renderer_preference,
            command_line: self.app.cfg.renderer_argument,
            ..self.app.present.renderer_status()
        }
    }

    /// Where the experimental rendering effects stand on the device the client draws with, for
    /// the options page.
    #[cfg(feature = "hifi")]
    #[must_use]
    pub fn hifi_availability(&self) -> dereth_client_contract::options::fidelity::Availability {
        self.app.present.hifi_availability()
    }

    /// Note whether the interface shown is Horizon, the only one the optional high-fidelity
    /// presentation draws under. The drawn world takes it at once (its next preference poll
    /// installs or removes the presentation), and every world built after it starts with it.
    /// A build without the presentation does nothing here.
    #[cfg_attr(not(feature = "hifi"), allow(clippy::unused_self))]
    pub fn set_hifi_interface(&mut self, horizon: bool) {
        #[cfg(feature = "hifi")]
        {
            if self.app.hifi_interface == horizon {
                return;
            }
            self.app.hifi_interface = horizon;
            let _ = self.app.present.apply_render_preference_requests(vec![
                dereth_client_contract::UiRequest::SetPreference(
                    dereth_client_contract::options::names::fidelity::INTERFACE,
                    dereth_client_contract::PrefValue::Bool(horizon),
                ),
            ]);
        }
        #[cfg(not(feature = "hifi"))]
        let _ = horizon;
    }

    /// A pad's two sticks under the orbit camera, as they are pushed this frame: `movement` moves
    /// the player (right and ahead, each from -1 to 1, `None` when it is let go; see
    /// [`crate::orbit::MovementKeys::stick`]), and `look` turns the camera (right and up, each from
    /// -1 to 1; see [`crate::orbit::OrbitCamera::turn_stick`]). Nothing without the orbit camera.
    pub fn orbit_sticks(&mut self, movement: Option<(f32, f32)>, look: (f32, f32)) {
        let Some(settings) = self.app.orbit else {
            return;
        };
        let asks = self.app.orbit_keys.stick(movement, settings);
        self.app.orbit_pending.extend(asks);
        // While a spell is cast the look stick turns the player across, the camera following;
        // it still tilts the camera.
        if self.app.orbit_keys.locked() && !self.app.orbit_keys.mouse_turns_player() {
            let turn = self.app.orbit_keys.cast_turn(look.0);
            self.app.orbit_pending.extend(turn);
            self.app.orbit_look = (0.0, look.1);
        } else {
            if !self.app.orbit_keys.mouse_turns_player() {
                let stop = self.app.orbit_keys.cast_turn(0.0);
                self.app.orbit_pending.extend(stop);
            }
            self.app.orbit_look = look;
        }
    }

    /// One of the orbit camera's mouse buttons, the right one for `right`, going down or up over
    /// the world. Either held turns the camera with the pointer; both together run the player
    /// forward where it looks. Under character-based movement the right one steers the player,
    /// and the turning keys step sideways while it is held.
    pub fn orbit_button(&mut self, right: bool, pressed: bool) {
        let keys = &mut self.app.orbit_keys;
        if right {
            keys.buttons.1 = pressed;
        } else {
            keys.buttons.0 = pressed;
        }
        let (left, right) = keys.buttons;
        self.app.mouse_look_button(left || right);
        if let Some(settings) = self.app.orbit {
            let run = self.app.orbit_keys.mouse_run(left && right, settings);
            self.app.orbit_pending.extend(run);
            let steps = self.app.orbit_keys.steering_changed(settings);
            self.app.orbit_pending.extend(steps);
        }
    }

    /// Draw every animated body between its animation's keyframes (`on`), or at them as the
    /// game draws them. Only the drawing changes; see
    /// `crate::world_state::WorldState::smooth_animation`.
    pub fn set_smooth_animation(&mut self, on: bool) {
        self.app.smooth_animation = on;
    }

    /// Draw every object physics or the server moves moving between its physics ticks (`on`), or
    /// where physics has it as the game draws it. Only the drawing changes; see
    /// `crate::world_state::WorldState::smooth_movement`.
    pub fn set_smooth_movement(&mut self, on: bool) {
        self.app.smooth_movement = on;
    }

    /// The orbit camera as the body's camera has it, when there is a body.
    #[must_use]
    pub fn orbit_camera(&self) -> Option<crate::orbit::OrbitCamera> {
        self.app
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(|c| c.camera.orbit)
    }

    /// The pointer moved, in window pixels.
    pub fn cursor_moved(&mut self, x: f64, y: f64) {
        self.app.cursor_moved(x, y);
    }

    /// The free camera's rise and sink keys.
    pub fn flycam_rise(&mut self, held: bool) {
        self.app.flycam_rise(held);
    }

    /// …
    pub fn flycam_sink(&mut self, held: bool) {
        self.app.flycam_sink(held);
    }
}

/// A UI's answer to one talk-focus notice: handed the current talk focus and the notice, it says
/// whether the talk focus falls back to All (the chat entry's row for the current focus was just
/// switched off under it).
pub type TalkFocusAnswer<'a> = dyn FnMut(TalkFocus, TalkFocusNotice) -> bool + 'a;

/// Talk-focus `notices` offered to a UI one at a time, in the order they were raised. A fall-back
/// the UI answers takes effect before the next notice is offered, which reads it.
pub fn offer_talk_focus_notices(
    chat: &mut ChatState,
    notices: Vec<TalkFocusNotice>,
    answer: &mut TalkFocusAnswer<'_>,
) {
    for notice in notices {
        let _ = answer(chat.talk_focus, notice);
    }
}
