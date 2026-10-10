//! The executable's front end: the modern UI, the cursor, the clipboard, the preview spaces and
//! the overlay, as [`ClientShell`], the [`Shell`] the executable supplies.
//!
//! It is one front end among any: it never holds the application. Each step of the frame it takes
//! part in hands it a [`UiContext`](dereth_client_runtime::ui_context::UiContext), and that is all
//! of the game it reaches. The application it plugs into is assembled in [`crate::app`].

use dereth_client_runtime::app::StartupError;
use dereth_client_runtime::shell::{Shell, UiNotices};
use dereth_primitives::DataId;

use crate::platform::host::Host;
use crate::present::ClientPresentation;

mod frame;
mod horizon;
mod horizon_keys;
mod key_bindings;
mod previews;
mod scripted_run;

use previews::ChargenPreviewKey;
use scripted_run::{PregameDrive, SayDrive, WorldDrive};

/// What the key-binding page's three host drains have done this session.
///
/// Every rate here carries its denominator: a page that builds **306 rows in a test and 0 in a
/// running client** reads the same as an idle one unless `0` is written as `0 of 306`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct KeyBindingStats {
    /// Key-binding page initializations, one per gameplay-screen construction.
    pub init_calls: u64,
    /// Rows the last options initialization built: the size of the page's option array.
    pub rows_built: usize,
    /// **The denominator**: the `(input map, action)` pairs of the merged action map the key
    /// pages list ([`dereth_input::presentation`]). `rows_built` must equal it.
    pub bindable_actions: usize,
    /// Section headers added (template 0), one per input map that contributed a row.
    pub headers: usize,
    /// Template-list item adds that produced nothing, and rows whose `post_init` failed.
    pub failures: usize,
    /// Controls diverted to the page.
    pub key_hits_offered: u64,
    /// …of which the page accepted, i.e. a row really was capturing. The two differ only when the
    /// handler registration and the page's own dialog context disagree, which is a bug if it ever
    /// happens and is why both are counted.
    pub key_hits_taken: u64,
    /// Element messages the drain dispatched into a row.
    pub row_events: u64,
    /// Capture verdicts the drain produced.
    pub captures: u64,
    /// …of which were `Capture::Ready`, i.e. a binding was actually written to the keymap.
    pub bindings_made: u64,
    /// Presses on the page's four action buttons that reached the page handler.
    pub page_button_events: u64,
    /// Rows wrote back — *Cancel* and *Revert to
    /// Saved*.
    pub rows_reverted: u64,
    /// Rows reset to registered defaults by *Restore Defaults*.
    pub rows_defaulted: u64,
    /// Rows snapshotted by *OK*.
    pub rows_applied: u64,
    /// Keymap writes made by *OK* when the page reported changed values.
    pub keymaps_written: u64,
}

/// Complete every request the UI raised, in order, at BOTH input-listener and HUD-update
/// boundaries: the UI's own owners first, then the runtime's ([`Cx::run_request`]) at once,
/// then the selection notices and the dialog service, so the game has seen each request before the
/// UI's next listener runs. A request's descendants finish before its next sibling.
fn dispatch_ui_owner_requests<H: Host>(
    shell: &mut crate::ui::UiShell,
    cx: &mut Cx<'_, H>,
    targeted_dialogs: &mut crate::target_confirmation::TargetedDialogs,
    serial: u64,
    now: dereth_primitives::LocalTime,
) -> Vec<dereth_ui_screens::view::UiRequest> {
    use dereth_client_runtime::interaction::TargetMode;
    use dereth_ui_screens::screens::gameplay_host::GameCall;
    use dereth_ui_screens::view::UiRequest;
    targeted_dialogs.service_with(cx, Some(shell), now);
    let active = cx.target_mode() != TargetMode::None;
    shell.set_target_mode_active(active);
    if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
        let (hud, objects) = cx.hud_and_objects();
        hud.dispatch_panel_input(&mut shell.ui, screen, serial, objects, active);
        crate::hud_drive::game_call(&mut shell.ui, screen, GameCall::CaptureBookDraft);
        hud.panels.journal.save_this_page(&mut shell.ui);
    }
    let mut unowned = Vec::new();
    loop {
        let pending = shell.ui.requests.take();
        if pending.is_empty() {
            break;
        }
        let mut pending: std::collections::VecDeque<_> = pending.into();
        while let Some(request) = pending.pop_front() {
            // The two requests that restyle the local player in place are the runtime's alone; the
            // rest are offered to the UI's own owners first.
            let request = match request {
                r @ (UiRequest::BarberLocalEffect(_) | UiRequest::BarberLocalMotionTable(_)) => {
                    unowned.extend(cx.run_request(r, now, &mut |_, _| false));
                    None
                }
                request => shell.handle_request(request),
            };
            if let Some(request) = request {
                unowned.extend(cx.run_request(request, now, &mut |focus, notice| {
                    crate::hud_drive::game_screen(&mut shell.flow).is_some_and(|screen| {
                        crate::hud_drive::talk_focus_notice(&mut shell.ui, screen, focus, notice)
                    })
                }));
            }
            for update in cx.take_chat_entry_updates() {
                if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                    crate::hud_drive::game_call(&mut shell.ui, screen, GameCall::ChatEntry(update));
                }
            }
            cx.deliver_selection_notices(now);
            targeted_dialogs.service_with(cx, Some(shell), now);
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    screen,
                    GameCall::InteractionContext {
                        target_mode: cx.target_mode() != TargetMode::None,
                        selected: cx.model().selected,
                    },
                );
                let (hud, objects) = cx.hud_and_objects();
                hud.refresh_item_input_views(&mut shell.ui, screen, objects);
            }
            // Selection -> split-size readback finishes before the next original sibling.
            for descendant in shell.ui.requests.take().into_iter().rev() {
                pending.push_front(descendant);
            }
        }
    }
    if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
        crate::hud_drive::game_call(
            &mut shell.ui,
            screen,
            GameCall::InteractionContext {
                target_mode: cx.target_mode() != TargetMode::None,
                selected: cx.model().selected,
            },
        );
        let (hud, objects) = cx.hud_and_objects();
        hud.refresh_item_input_views(&mut shell.ui, screen, objects);
    }
    if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
        let GameCall::ChatDrafts(drafts) =
            crate::hud_drive::game_call(&mut shell.ui, screen, GameCall::ChatDrafts(Vec::new()))
        else {
            unreachable!()
        };
        for (window, text) in drafts {
            if cx
                .model()
                .chat
                .entries
                .get(&window)
                .map(|entry| entry.text.as_str())
                != Some(text.as_str())
            {
                unowned.extend(cx.run_request(
                    UiRequest::ChatEntry {
                        window,
                        text,
                        action: dereth_client_contract::chat::entry::EntryAction::Draft,
                    },
                    now,
                    &mut |_, _| false,
                ));
            }
        }
    }
    shell.set_target_mode_active(cx.target_mode() != TargetMode::None);
    unowned
}

/// The executable's front end: everything the frame hands the UI, and the state the UI keeps
/// across frames.
pub struct ClientShell<H: Host> {
    pub(crate) modern: ModernFrontEnd,
    pub(crate) shared: FrontEndServices<H>,
    pub(crate) classic: crate::classic_face::ClassicFace,
    pub(crate) horizon: horizon::HorizonFace,
}

/// Widget state, dialogs and preview caches owned by the modern interface.
#[derive(Debug)]
pub(crate) struct ModernFrontEnd {
    /// How far `--enter-world` has driven the pre-game screens. See `App::drive_pregame_screens`.
    pregame: PregameDrive,
    /// How far `--cast` has driven the world screen. See `Ui::drive_world_script`.
    world_drive: WorldDrive,
    /// How far `--say` and `--use` have got. See `Ui::drive_say`.
    say_drive: SayDrive,

    /// the current cursor id plus the cursor image. `None` without `--ui`.
    pub(crate) ui: Option<crate::ui::UiShell>,
    targeted_dialogs: crate::target_confirmation::TargetedDialogs,
    resolution_dialog: dereth_ui::dialog::resolution::ResolutionDialog,
    /// How many times the gameplay screen has been constructed, so the HUD's
    /// player-module refresh runs again on a tree that was rebuilt by a mode switch.
    gameplay_serial: u64,

    /// The [`Self::gameplay_serial`] was last run for.
    ///
    /// Option initialization **flushes every list box** and rebuilds the option rows from scratch, so it
    /// is a per-*screen* call and not a per-frame one — running it every frame would throw away
    /// the row a player was in the middle of rebinding. Screen construction is where the client
    /// would do it; this build waits until here because every row reads the merged
    /// master input map out of the host's input manager, which screen creation is not given.
    key_bindings_built: Option<u64>,
    /// What the three drains have done, with denominators. See [`KeyBindingStats`].
    pub(crate) key_binding_stats: KeyBindingStats,
    /// Time at the previous character-generation preview tick. Preview animations advance from
    /// **elapsed seconds** and accumulate nothing per frame; see
    /// [`dereth_scene::preview::PreviewSpace::use_time`].
    preview_last_time: f64,
    /// Time at the previous paper-doll preview tick.
    paper_doll_last_time: f64,
    /// Time at the previous identify-window portrait tick.
    examine_3d_last_time: f64,
    /// Whether the creation wizard was the screen last frame, so its edges reach the runtime as
    /// `UiRequest::CharacterCreation`.
    in_creation: bool,
    /// What the identify window's space was last built for: the appraised object and the setup record
    /// it was wearing.
    ///
    /// Appraisal delivery requests a rebuild once per `0x00C9` reply rather than once per frame.
    /// This field records that "once" as the identity
    /// of what was built — the object id **and** its setup, so that a creature whose body changes
    /// under a live panel is rebuilt rather than left stale.
    examine_3d_built: Option<(dereth_primitives::ObjectId, DataId)>,
    /// What the 3D character preview's space was last built for. See [`ChargenPreviewKey`].
    preview_chargen: Option<ChargenPreviewKey>,
    /// Palette-set answers memoised across repaints -- the one dat read the
    /// `ObjDesc` block makes. The client looks up `(id, 0x18)` and lets the object cache absorb it.
    chargen_pal_sets: dereth_scene::preview::PaletteSetCache,
    /// What the last `chargen_objdesc` reached, so a test can assert the block ran
    /// rather than inferring it from an empty descriptor. See
    /// [`dereth_scene::preview::ChargenDressStats`].
    pub(crate) chargen_dress: dereth_scene::preview::ChargenDressStats,
    /// What the paper doll was last built for: the player's
    /// setup record and the `ObjDesc` used to redress the creature.
    ///
    /// The client keeps the clone for ever and reapplies only descriptor changes when the
    /// appearance changes; here the descriptor is part of the key because the part meshes are
    /// baked from the dressed array (see
    /// [`dereth_scene::preview::PreviewSpace::add_object_dressed`]), so a redress is a rebuild. The
    /// flag is whether it wears another era's look (`[Render] Objects`), so switching the look
    /// rebuilds it too.
    paper_doll_built: Option<(DataId, dereth_animation::parts::ObjDesc, bool)>,
    /// The paper-doll panel's flip count, next-flip time, and selection mask —
    /// the doll's selection blink. See [`dereth_scene::preview::PaperDollSelectionLighting`].
    paper_doll_lighting: dereth_scene::preview::PaperDollSelectionLighting,
    /// The selection last observed by the paper doll. `dereth_client_model` records selection broadcasts
    /// instead of raising them directly, so this edge is delivered with the same one-frame delay
    /// as the other subscriber. Re-selecting the unchanged id broadcasts without an edge and does
    /// not restart the blink.
    paper_doll_selection_seen: Option<dereth_primitives::ObjectId>,
}

/// Host services shared by the currently active interface.
pub(crate) struct FrontEndServices<H: Host> {
    /// The client UI cursor half: the current cursor id, the built
    /// `HCURSOR`s, and the window they are installed on. See [`crate::cursor`].
    pub(crate) cursor: crate::cursor::CursorSystem,
    /// The clipboard bridge: the Win32 hop `dereth-ui` cannot make, and the
    /// sequence number that keeps the per-frame refresh from taking the desktop clipboard lock.
    clipboard: crate::clipboard::ClipboardBridge,
    /// The host's clipboard, which the bridge mirrors.
    host_clipboard: H::Clipboard,
    /// The host's pad, read once a frame.
    gamepad: Box<dyn crate::gamepad::HostGamepad>,
    /// The pointer, held by the host during a camera drag.
    pub(crate) pointer: crate::pointer::PointerHold,
    /// A pad state set by an in-process driver, read in place of the host's pad while it is set.
    pub(crate) scripted_pad: Option<crate::gamepad::PadState>,
    /// The device input: the input manager and the registrations the client's systems make.
    /// Present whenever the input tables loaded, with or without `--ui`.
    pub(crate) input: Option<crate::input::InputShell>,
    /// The device half of the message mapping: the Alt state and the pointer position the button
    /// messages carry.
    devices: dereth_input::pump::DeviceMessages,
    /// The window's queue of events, routed at the event-loop step.
    window_events: crate::platform::window::WindowEvents,
    /// What every `release_ui_textures` has done so far. `unknown` is a double release and is
    /// asserted zero.
    pub(crate) ui_release: crate::gpu::UiReleaseReport,
    /// This frame's 2D blit list, built at step 7 and drawn inside `PresentFrame`.
    pub(crate) ui_draw_list: Vec<dereth_ui::UiDrawCmd>,
    /// The reopen of the data files the interfaces last read them at
    /// ([`dereth_client_runtime::ui_context::UiContext::store_generation`]).
    files_read: u64,
}

impl<H: Host> std::fmt::Debug for ClientShell<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientShell")
            .field("ui", &self.modern.ui.is_some())
            .field("gameplay_serial", &self.modern.gameplay_serial)
            .finish_non_exhaustive()
    }
}

impl<H: Host> ClientShell<H> {
    /// A front end whose cursor is installed on `hwnd` by host `H`, with no UI up yet.
    #[must_use]
    pub fn new(hwnd: Option<isize>) -> Self {
        Self::with_window_events(hwnd, crate::platform::window::WindowEvents::default())
    }

    /// Queue a host event as the window would, to be routed with the next frame's: the way an
    /// in-process driver gives the client real input.
    pub fn queue_window_event(&self, event: dereth_input::host::HostEvent) {
        self.shared.window_events.borrow_mut().push(event);
    }

    /// A front end that routes the events the window queues on `window_events`.
    #[must_use]
    pub fn with_window_events(
        hwnd: Option<isize>,
        window_events: crate::platform::window::WindowEvents,
    ) -> Self {
        Self {
            modern: ModernFrontEnd {
                pregame: PregameDrive::default(),
                world_drive: WorldDrive::default(),
                say_drive: SayDrive::default(),
                ui: None,
                targeted_dialogs: crate::target_confirmation::TargetedDialogs::default(),
                resolution_dialog: dereth_ui::dialog::resolution::ResolutionDialog::default(),
                gameplay_serial: 0,
                key_bindings_built: None,
                key_binding_stats: KeyBindingStats::default(),
                preview_last_time: 0.0,
                paper_doll_last_time: 0.0,
                examine_3d_last_time: 0.0,
                in_creation: false,
                examine_3d_built: None,
                preview_chargen: None,
                chargen_pal_sets: dereth_scene::preview::PaletteSetCache::default(),
                chargen_dress: dereth_scene::preview::ChargenDressStats::default(),
                paper_doll_built: None,
                paper_doll_lighting: dereth_scene::preview::PaperDollSelectionLighting::default(),
                paper_doll_selection_seen: None,
            },
            shared: FrontEndServices {
                input: None,
                devices: dereth_input::pump::DeviceMessages::default(),
                window_events,
                cursor: crate::cursor::CursorSystem::with_images(H::cursor_images(hwnd)),
                clipboard: crate::clipboard::ClipboardBridge::default(),
                host_clipboard: H::clipboard(),
                gamepad: H::gamepad(),
                pointer: crate::pointer::PointerHold::new(H::pointer(hwnd)),
                scripted_pad: None,
                ui_release: crate::gpu::UiReleaseReport::default(),
                ui_draw_list: Vec::new(),
                files_read: 0,
            },
            classic: crate::classic_face::ClassicFace::default(),
            horizon: horizon::HorizonFace::default(),
        }
    }
}

/// An interface owns its widgets and transient presentation state. Runtime and host
/// services are borrowed for a call and never contain the owning shell.
trait FrontEnd<H: Host> {
    fn game_viewport(&self) -> Option<dereth_primitives::Viewport>;
    fn pointer_over_game_view(&self, cursor: (i32, i32)) -> bool;
    fn examine_panel_open(&mut self) -> bool;
    fn close_examine_panel(&mut self);
    fn service_dialogs(&mut self, cx: &mut Cx<'_, H>, now: dereth_primitives::LocalTime);
    fn before_ui_input(&mut self, player_airborne: bool);
    fn talk_focus_notice(
        &mut self,
        talk_focus: dereth_client_model::chat::TalkFocus,
        notice: dereth_client_model::chat::TalkFocusNotice,
    ) -> bool;
    fn deliver_power_bar_notices(
        &mut self,
        hud: &mut crate::hud::Hud,
        notices: Vec<dereth_client_model::combat::PowerBarNotice>,
    );
    fn object_panel_notice(
        &mut self,
        hud: &mut crate::hud::Hud,
        world: &dereth_client_model::World,
        notice: &dereth_client_model::Notice,
    ) -> Vec<dereth_client_contract::UiRequest>;
    fn emit_magic_notices(&mut self, notices: Vec<dereth_client_contract::view::MagicNotice>);
    fn open_vendor_buying(&mut self, hud: &mut crate::hud::Hud);
    fn run_ui_layout_commands(
        &mut self,
        prefs: &std::path::Path,
        character: &str,
        world: &str,
        layout_commands: Vec<dereth_client_runtime::interaction::UiLayoutCommand>,
    );
    fn split_stack(
        &mut self,
        view: &dereth_client_runtime::hud::HudView<'_>,
        selected: dereth_primitives::ObjectId,
    );
    fn dispatch_input_action(&mut self, action: u32) -> Option<bool>;
    fn world_tooltip(&mut self, tooltip: dereth_client_runtime::interaction::WorldTooltip);
    fn chat_generation(&self) -> Option<u64>;

    fn in_gameplay(&self) -> bool;
    fn hides_world(&self) -> bool;
    fn requests(&mut self) -> Option<&mut dereth_client_contract::requests::Outbox>;
    fn frame(
        &mut self,
        cx: &mut Cx<'_, H>,
        services: &mut FrontEndServices<H>,
        now: dereth_primitives::LocalTime,
        notices: UiNotices,
    ) -> bool;
    fn before_portal(&mut self, cx: &mut Cx<'_, H>, services: &mut FrontEndServices<H>);
    fn after_portal(&mut self, cx: &mut Cx<'_, H>, services: &mut FrontEndServices<H>);
    fn compose(&mut self, cx: &mut Cx<'_, H>, services: &mut FrontEndServices<H>);
    fn draw(
        &mut self,
        present: &mut dyn ClientPresentation,
        services: &FrontEndServices<H>,
    ) -> Result<(), dereth_client_runtime::present::PresentError>;
    fn resize(&mut self, display: (i32, i32));
    fn suspend(&mut self, cx: &mut Cx<'_, H>);
    /// Read the data files again, as a data patch left them: what the interface keeps from them
    /// is read from [`Cx::store`] now, and what it uploaded from the old records is let go.
    fn reread_files(&mut self, cx: &mut Cx<'_, H>, services: &mut FrontEndServices<H>);
}

impl<H: Host> FrontEnd<H> for ModernFrontEnd {
    fn reread_files(&mut self, cx: &mut Cx<'_, H>, services: &mut FrontEndServices<H>) {
        let Some(shell) = self.ui.as_mut() else {
            return;
        };
        if let Err(e) = shell.reread_files(cx.store()) {
            tracing::warn!("the interface could not read the patched data files ({e}); it keeps the ones it had");
            return;
        }
        let r = cx.present_mut().release_ui_textures();
        services.ui_release.freed += r.freed;
        services.ui_release.still_linked += r.still_linked;
        services.ui_release.unknown += r.unknown;
        // The preview spaces are built again from the new records the next time they are drawn.
        self.preview_chargen = None;
        self.chargen_pal_sets = dereth_scene::preview::PaletteSetCache::default();
        self.paper_doll_built = None;
        self.examine_3d_built = None;
    }

    fn game_viewport(&self) -> Option<dereth_primitives::Viewport> {
        let shell = self.ui.as_ref()?;
        let root = *shell.flow.current()?.roots().first()?;
        let h = shell
            .ui
            .get_child_recursive(root, dereth_ui_screens::hud::world_view::SMART_BOX)?;
        let b = dereth_ui_screens::hud::world_view::client_rect(&shell.ui, h);
        if !b.is_valid() {
            return None;
        }
        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: `is_valid` proves the box is non-empty; `max(0)` covers an element laid out off
        // the top-left edge, matching the UI box clamp used during layout.
        Some(dereth_primitives::Viewport {
            x: b.x0.max(0) as u32,
            y: b.y0.max(0) as u32,
            width: (b.x1 - b.x0.max(0) + 1).max(0) as u32,
            height: (b.y1 - b.y0.max(0) + 1).max(0) as u32,
        })
    }
    fn pointer_over_game_view(&self, cursor: (i32, i32)) -> bool {
        let Some(shell) = self.ui.as_ref() else {
            return true;
        };
        let (x, y) = cursor;
        pointer_over_game_view_at(shell, shell.ui.hit_test_screen(x, y))
    }
    fn examine_panel_open(&mut self) -> bool {
        let Some(shell) = self.ui.as_mut() else {
            return false;
        };
        let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) else {
            return false;
        };
        use dereth_ui_screens::screens::gameplay_host::GameCall;
        matches!(
            crate::hud_drive::game_call(&mut shell.ui, screen, GameCall::ExaminationOpen(false)),
            GameCall::ExaminationOpen(true)
        )
    }
    fn close_examine_panel(&mut self) {
        let Some(shell) = self.ui.as_mut() else {
            return;
        };
        let ui = &mut shell.ui;
        let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) else {
            return;
        };
        crate::hud_drive::game_call(
            ui,
            screen,
            dereth_ui_screens::screens::gameplay_host::GameCall::CloseExamination(false),
        );
    }
    fn service_dialogs(&mut self, cx: &mut Cx<'_, H>, now: dereth_primitives::LocalTime) {
        self.targeted_dialogs
            .service_with(cx, self.ui.as_mut(), now);
    }
    fn before_ui_input(&mut self, player_airborne: bool) {
        if let Some(shell) = self.ui.as_mut() {
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    screen,
                    dereth_ui_screens::screens::gameplay_host::GameCall::PlayerAirborne(
                        player_airborne,
                    ),
                );
            }
        }
    }
    fn talk_focus_notice(
        &mut self,
        talk_focus: dereth_client_model::chat::TalkFocus,
        notice: dereth_client_model::chat::TalkFocusNotice,
    ) -> bool {
        self.ui.as_mut().is_some_and(|shell| {
            crate::hud_drive::game_screen(&mut shell.flow).is_some_and(|screen| {
                crate::hud_drive::talk_focus_notice(&mut shell.ui, screen, talk_focus, notice)
            })
        })
    }
    fn deliver_power_bar_notices(
        &mut self,
        hud: &mut crate::hud::Hud,
        notices: Vec<dereth_client_model::combat::PowerBarNotice>,
    ) {
        let Some(shell) = self.ui.as_mut() else {
            return;
        };
        let Some(screen) = shell.flow.current() else {
            return;
        };
        if !screen.is_game() {
            return;
        }
        let writes = u64::from(crate::hud_drive::deliver_power_bar_notices(
            &mut shell.ui,
            &mut hud.panels,
            notices,
        ));
        hud.stats.power_bar_writes += writes;
        hud.stats.panels_written += writes;
    }
    fn object_panel_notice(
        &mut self,
        hud: &mut crate::hud::Hud,
        world: &dereth_client_model::World,
        notice: &dereth_client_model::Notice,
    ) -> Vec<dereth_client_contract::UiRequest> {
        dispatch_object_panel_notice(self.ui.as_mut(), hud, world, notice)
    }
    fn emit_magic_notices(&mut self, notices: Vec<dereth_client_contract::view::MagicNotice>) {
        if let Some(shell) = self.ui.as_mut() {
            for n in notices {
                shell.ui.notice_inbox.emit(n);
            }
        }
    }
    fn open_vendor_buying(&mut self, hud: &mut crate::hud::Hud) {
        if let Some(shell) = self.ui.as_mut() {
            hud.panels.vendor.open_buying(&mut shell.ui);
        }
    }
    fn run_ui_layout_commands(
        &mut self,
        prefs: &std::path::Path,
        character: &str,
        world: &str,
        layout_commands: Vec<dereth_client_runtime::interaction::UiLayoutCommand>,
    ) {
        if let Some(shell) = self.ui.as_mut() {
            for command in layout_commands {
                let result = match command {
                    dereth_client_runtime::interaction::UiLayoutCommand::Save(name) => shell
                        .screen_layout_path(&name, prefs, character, world)
                        .map(|path| shell.save_ui_layout(&path)),
                    dereth_client_runtime::interaction::UiLayoutCommand::Load(name) => shell
                        .screen_layout_path(&name, prefs, character, world)
                        .map(|path| shell.load_ui_layout(&path)),
                    dereth_client_runtime::interaction::UiLayoutCommand::SetLockUi(locked) => {
                        // Both `/lockui` and the radar request already ran the lock-UI setter, then
                        // OnChanged(51) in Interaction. Complete native's following global-0D
                        // visible cascade without constructing a second option write.
                        shell.apply_lock_ui(locked);
                        None
                    }
                };
                if let Some(Err(e)) = result {
                    tracing::warn!("screen layout command failed: {e}");
                }
            }
        }
    }
    fn split_stack(
        &mut self,
        view: &dereth_client_runtime::hud::HudView<'_>,
        selected: dereth_primitives::ObjectId,
    ) {
        if let Some(shell) = self.ui.as_mut() {
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                crate::hud_drive::game_call_with_view(
                    &mut shell.ui,
                    screen,
                    view,
                    dereth_ui_screens::screens::gameplay_host::GameCall::SplitStack(selected),
                );
            }
        }
    }
    fn dispatch_input_action(&mut self, action: u32) -> Option<bool> {
        self.ui
            .as_mut()
            .map(|shell| shell.ui.dispatch_input_action(action))
    }
    fn world_tooltip(&mut self, tooltip: dereth_client_runtime::interaction::WorldTooltip) {
        if let Some(shell) = self.ui.as_mut() {
            apply_world_tooltip(shell, tooltip);
        }
    }
    fn chat_generation(&self) -> Option<u64> {
        self.ui
            .as_ref()
            .and_then(|shell| shell.flow.current())
            .and_then(|s| s.is_game().then_some(self.gameplay_serial))
    }

    fn in_gameplay(&self) -> bool {
        self.ui
            .as_ref()
            .is_some_and(|s| s.flow.current_mode() == Some(dereth_ui::framework::mode::GAME_PLAY))
    }
    fn hides_world(&self) -> bool {
        self.ui.as_ref().and_then(|s| s.flow.current_mode())
            == Some(dereth_ui::framework::mode::CREDITS)
    }
    fn requests(&mut self) -> Option<&mut dereth_client_contract::requests::Outbox> {
        self.ui.as_mut().map(|s| &mut s.ui.requests)
    }
    fn frame(
        &mut self,
        cx: &mut Cx<'_, H>,
        services: &mut FrontEndServices<H>,
        now: dereth_primitives::LocalTime,
        notices: UiNotices,
    ) -> bool {
        Ui {
            cx,
            front: self,
            shared: services,
        }
        .ui_frame(now, notices)
    }
    fn before_portal(&mut self, cx: &mut Cx<'_, H>, services: &mut FrontEndServices<H>) {
        let mut ui = Ui {
            cx,
            front: self,
            shared: services,
        };
        ui.own_actions();
        ui.preview_use_time();
    }
    fn after_portal(&mut self, cx: &mut Cx<'_, H>, services: &mut FrontEndServices<H>) {
        let mut ui = Ui {
            cx,
            front: self,
            shared: services,
        };
        ui.paper_doll_use_time();
        ui.examine_3d_use_time();
    }
    fn compose(&mut self, cx: &mut Cx<'_, H>, services: &mut FrontEndServices<H>) {
        Ui {
            cx,
            front: self,
            shared: services,
        }
        .compose_ui_draw_list();
    }
    fn draw(
        &mut self,
        present: &mut dyn ClientPresentation,
        services: &FrontEndServices<H>,
    ) -> Result<(), dereth_client_runtime::present::PresentError> {
        present.draw_ui(&services.ui_draw_list)
    }
    fn resize(&mut self, display: (i32, i32)) {
        if let Some(ui) = &mut self.ui {
            ui.set_display(display);
        }
    }
    fn suspend(&mut self, _cx: &mut Cx<'_, H>) {
        if let Some(ui) = &mut self.ui {
            self.resolution_dialog.project(&mut ui.ui, None);
            // The interface choice is made on a press, so the button can still be down: its
            // release reaches the other interface, and the press ends here.
            ui.release_pointer();
        }
    }
}

impl<H: Host> FrontEnd<H> for dereth_classic_ui::runtime::ClassicUi {
    fn reread_files(&mut self, cx: &mut Cx<'_, H>, _services: &mut FrontEndServices<H>) {
        dereth_classic_ui::runtime::ClassicUi::reread_files(self, cx);
    }

    fn game_viewport(&self) -> Option<dereth_primitives::Viewport> {
        self.game_viewport()
    }
    fn pointer_over_game_view(&self, cursor: (i32, i32)) -> bool {
        self.pointer_over_game_view(cursor)
    }
    fn examine_panel_open(&mut self) -> bool {
        dereth_classic_ui::runtime::ClassicUi::examine_panel_open(self)
    }
    fn close_examine_panel(&mut self) {
        self.close_examine_panel();
    }
    fn service_dialogs(&mut self, cx: &mut Cx<'_, H>, now: dereth_primitives::LocalTime) {
        self.service_dialogs(cx, now);
    }
    fn before_ui_input(&mut self, player_airborne: bool) {
        let _ = player_airborne;
    }
    fn talk_focus_notice(
        &mut self,
        talk_focus: dereth_client_model::chat::TalkFocus,
        notice: dereth_client_model::chat::TalkFocusNotice,
    ) -> bool {
        self.talk_focus_notice(talk_focus, notice)
    }
    fn deliver_power_bar_notices(
        &mut self,
        hud: &mut crate::hud::Hud,
        notices: Vec<dereth_client_model::combat::PowerBarNotice>,
    ) {
        let _ = hud;

        self.deliver_power_bar_notices(notices);
    }
    fn object_panel_notice(
        &mut self,
        hud: &mut crate::hud::Hud,
        world: &dereth_client_model::World,
        notice: &dereth_client_model::Notice,
    ) -> Vec<dereth_client_contract::UiRequest> {
        let _ = hud;

        self.object_panel_notice(world, notice)
    }
    fn emit_magic_notices(&mut self, notices: Vec<dereth_client_contract::view::MagicNotice>) {
        self.emit_magic_notices(notices);
    }
    fn open_vendor_buying(&mut self, hud: &mut crate::hud::Hud) {
        let _ = hud;

        self.open_vendor_buying();
    }
    fn run_ui_layout_commands(
        &mut self,
        prefs: &std::path::Path,
        character: &str,
        world: &str,
        layout_commands: Vec<dereth_client_runtime::interaction::UiLayoutCommand>,
    ) {
        let _ = (prefs, character, world, layout_commands);
    }
    fn split_stack(
        &mut self,
        view: &dereth_client_runtime::hud::HudView<'_>,
        selected: dereth_primitives::ObjectId,
    ) {
        let _ = (view, selected);
    }
    fn dispatch_input_action(&mut self, action: u32) -> Option<bool> {
        self.dispatch_input_action(action)
    }
    fn world_tooltip(&mut self, tooltip: dereth_client_runtime::interaction::WorldTooltip) {
        let _ = tooltip;
    }
    fn chat_generation(&self) -> Option<u64> {
        self.in_gameplay().then_some(0)
    }

    fn in_gameplay(&self) -> bool {
        self.in_gameplay()
    }
    fn hides_world(&self) -> bool {
        self.hides_world()
    }
    fn requests(&mut self) -> Option<&mut dereth_client_contract::requests::Outbox> {
        Some(&mut self.outbox)
    }
    fn frame(
        &mut self,
        cx: &mut Cx<'_, H>,
        services: &mut FrontEndServices<H>,
        now: dereth_primitives::LocalTime,
        notices: UiNotices,
    ) -> bool {
        self.ui_frame(cx, now, notices);
        let requests = self.take_key_store_requests();
        if !requests.is_empty() {
            if let Some(input) = services.input.as_mut() {
                for request in requests {
                    input.classic_request(request);
                }
            }
            self.set_classic_keys(classic_keys(services.input.as_mut()));
        }
        false
    }
    fn before_portal(&mut self, _cx: &mut Cx<'_, H>, _services: &mut FrontEndServices<H>) {}
    fn after_portal(&mut self, _cx: &mut Cx<'_, H>, _services: &mut FrontEndServices<H>) {}
    fn compose(&mut self, cx: &mut Cx<'_, H>, _services: &mut FrontEndServices<H>) {
        self.compose_ui(cx);
        for error in self.errors.drain(..).chain(self.desktop.errors.drain(..)) {
            tracing::warn!("classic interface: {error}");
        }
    }
    fn draw(
        &mut self,
        present: &mut dyn ClientPresentation,
        _services: &FrontEndServices<H>,
    ) -> Result<(), dereth_client_runtime::present::PresentError> {
        self.draw_ui(present)
    }
    fn resize(&mut self, display: (i32, i32)) {
        self.set_display(display);
    }
    fn suspend(&mut self, cx: &mut Cx<'_, H>) {
        self.suspend(cx);
    }
}

/// The character option word's automatic-shortcuts bit, which the early clients' option named.
const AUTO_CREATE_SHORTCUTS_BIT: u32 = 1;

/// What this executable's front end is handed at each step of the frame.
type Cx<'a, H> = dereth_client_runtime::ui_context::UiContext<'a, ClientShell<H>>;

/// The UI's steps, with the step's context and the front end both in hand.
struct Ui<'a, 'c, H: Host> {
    cx: &'a mut Cx<'c, H>,
    front: &'a mut ModernFrontEnd,
    shared: &'a mut FrontEndServices<H>,
}

impl<H: Host> Ui<'_, '_, H> {
    fn input_message(&mut self) {
        let (Some(shell), Some(input)) = (self.front.ui.as_mut(), self.shared.input.as_mut())
        else {
            return;
        };
        input.collect_message();
        let now = dereth_primitives::LocalTime(self.cx.now());
        let cx = &mut *self.cx;
        let targeted = &mut self.front.targeted_dialogs;
        let serial = self.front.gameplay_serial;
        if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
            use dereth_ui_screens::screens::gameplay_host::GameCall;
            crate::hud_drive::game_call(
                &mut shell.ui,
                screen,
                GameCall::AutoTargetWorld(cx.hud().auto_target_world(cx.model())),
            );
            crate::hud_drive::game_call(
                &mut shell.ui,
                screen,
                GameCall::ChatState(cx.hud().chat_focus_view(cx.model())),
            );
        }
        let mut unowned = Vec::new();
        shell.message_with_dispatch(now, input, &mut |shell, point| {
            if let crate::ui::UiDispatch::Mouse(event) = point {
                cx.pointer(event);
                return;
            }
            unowned.extend(dispatch_ui_owner_requests(shell, cx, targeted, serial, now));
        });
        input.finish_session_retirement(|released| {
            for action in released {
                cx.inject_action(action);
            }
        });
        cx.queue(Vec::new(), unowned);
        self.drive_key_bindings();
        self.own_actions();
        if let Some(input) = self.shared.input.as_mut() {
            if let Some(ui) = self.front.ui.as_mut() {
                ui.sync_input_scope(input);
            }
            input.defer_declined_actions();
        }
    }

    /// This client's own actions that the modern interface answers itself, taken out of what the
    /// screens left this frame (the performance panel's key and hold sidestep go on to the
    /// runtime, which answers them in either interface):
    ///
    /// | action | what this interface does |
    /// |---|---|
    /// | trade, spell research | the secure-trade window; the magic window's Create Spell tab |
    /// | automatic shortcuts | flips the character's option, as the classic key does |
    /// | inverted mouse look, mute when inactive | flips the shared preference |
    /// | right-click mouse look, stretched interface | flips the classic interface's own setting and says so: this interface has no such mode |
    fn own_actions(&mut self) {
        use dereth_client_contract::actions::dereth as own;
        let Some(input) = self.shared.input.as_mut() else {
            return;
        };
        let (mine, rest): (Vec<_>, Vec<_>) = input.take_events().into_iter().partition(|e| {
            e.input_map == dereth_input::dereth::INPUT_MAP
                && e.action != own::TOGGLE_PERFORMANCE_PANEL
                && e.action != own::MOVEMENT_HOLD_SIDESTEP
        });
        input.put_back_unconsumed(rest);
        for e in mine {
            let id = e.action;
            if !e.start {
                continue;
            }
            match id {
                own::TOGGLE_TRADE_PANEL | own::TOGGLE_SPELL_RESEARCH_PANEL => {
                    if let Some(shell) = self.front.ui.as_mut() {
                        if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                            crate::hud_drive::game_call(
                                &mut shell.ui,
                                screen,
                                dereth_ui_screens::screens::gameplay_host::GameCall::OwnWindowAction(
                                    id.0, false,
                                ),
                            );
                        }
                    }
                }
                own::PLAYER_OPTION_AUTO_CREATE_SHORTCUTS => {
                    let options = &self.cx.model().player_system.options;
                    let request = dereth_client_contract::UiRequest::SetOptionWords {
                        options: options.options ^ AUTO_CREATE_SHORTCUTS_BIT,
                        options2: options.options2,
                        timestamp_format: None,
                        save: true,
                    };
                    let now = dereth_primitives::LocalTime(self.cx.now());
                    let _ = self.cx.run_request(request, now, &mut |_, _| false);
                }
                own::TOGGLE_INVERT_MOUSE_LOOK => {
                    self.flip_preference(
                        dereth_client_contract::options::names::INVERT_MOUSE_LOOK_Y_AXIS,
                        None,
                    );
                }
                own::TOGGLE_MUTE_ON_LOSING_FOCUS => {
                    self.flip_preference(
                        dereth_client_contract::options::names::PLAY_SOUND_ONLY_WHEN_ACTIVE,
                        None,
                    );
                }
                own::TOGGLE_RIGHT_CLICK_MOUSE_LOOK => self.flip_preference(
                    dereth_client_contract::options::classic::RIGHT_CLICK_MOUSE_LOOK,
                    Some("Right-click mouse look"),
                ),
                own::TOGGLE_STRETCH_UI => self.flip_preference(
                    dereth_client_contract::options::classic::STRETCH_UI,
                    Some("The stretched interface"),
                ),
                _ => {}
            }
        }
    }

    /// Flip a yes-or-no preference of the shared store, live. A setting of the classic interface's
    /// own (`classic` names it) is only stored, and the chat says so.
    fn flip_preference(&mut self, name: &'static str, classic: Option<&str>) {
        use dereth_client_contract::PrefValue;
        let on = matches!(
            dereth_client_contract::options::store::inq_value(name),
            Some(PrefValue::Bool(true))
        );
        let value = PrefValue::Bool(!on);
        match classic {
            None => {
                // Stored, as the options page stores it, then applied live.
                let _ = dereth_client_contract::options::store::set_value(name, value.clone());
                let now = dereth_primitives::LocalTime(self.cx.now());
                let _ = self.cx.run_request(
                    dereth_client_contract::UiRequest::SetPreference(name, value),
                    now,
                    &mut |_, _| false,
                );
            }
            Some(what) => {
                let _ = dereth_client_contract::options::store::set_value(name, value);
                let state = if on { "off" } else { "on" };
                self.cx.add_scroll_line(
                    &format!("{what} is {state} in the classic interface."),
                    dereth_client_model::scroll::LOCAL_ERROR_TYPE,
                );
            }
        }
    }

    /// Bring the UI up, after input and sound.
    fn start_ui(&mut self) -> Result<(), StartupError> {
        // Initialize the UI after input and sound.
        if self.cx.config().ui {
            let (w, h) = self.cx.present().size();
            let display = (
                i32::try_from(w).unwrap_or(i32::MAX),
                i32::try_from(h).unwrap_or(i32::MAX),
            );
            let mut shell = crate::ui::UiShell::new(self.cx.store(), display).map_err(|e| {
                StartupError::Device {
                    cause: format!("InitUI: {e}"),
                }
            })?;
            // Load user preferences into the option store the configuration page actually reads;
            // without this the page does not reflect the saved state.
            //
            // `UiShell::new` runs `init_ui_preferences` -> `store::init`, which clears the registry
            // and repopulates it from the 34 hardcoded `registered_default` constants, so without
            // `options::store::load` every page would open showing a compiled-in default no matter
            // what the file said. Writes are a separate path: the page updates both the store and
            // its owning subsystem through `UiRequest::SetPreference`.
            //
            // **The order here is inverted relative to retail and has to be.** Retail calls
            // preference loading as the *first* line of initialization,
            // and every preference registration after it reads the already-loaded shadow value into
            // its destination variable, so the file wins over the registered default. This build's
            // registry is rebuilt by `UiShell::new`, so the file has to be re-applied after it to
            // reach the same end state. `seed_ui_registry` still follows, because a preference with
            // a live owner is authoritative over the file — that is what the renderer's actual
            // sampler state is.
            // Initialize the two
            // `Display.*` choice lists, built out of the enumerated adapter modes. It runs here
            // for the same reason `store::load` does: `UiShell::new` -> `store::init` has just
            // cleared and re-registered every variable, and `Display.Resolution` needs its
            // choice list before the file is pushed in (a saved `Resolution=1280x720` resolves
            // through the label list) and before the options page is ever built (that is the
            // drop-down's contents).
            //
            // In the shipped client, display choices are initialized before preference variables
            // are registered, so each registration receives its preloaded shadow value. This
            // build's registry is rebuilt by the shell, so both initialization steps follow it.
            self.cx.start_preferences();
            // `--object-visuals` wins over the profile's object mode, and the options page shows
            // what is drawn.
            if let Some(style) = self.cx.config().object_visuals {
                let _ = dereth_client_contract::options::store::set_value(
                    dereth_client_contract::options::landscape::OBJECTS,
                    dereth_client_contract::PrefValue::Int(style.map_or(
                        dereth_client_contract::options::landscape::WORLD_DEFAULT,
                        dereth_client_contract::options::landscape::RegionStyle::value,
                    )),
                );
            }
            dereth_client_runtime::render_prefs::seed_ui_registry(
                self.cx.present().texture_filtering(),
            );
            // The data-movie loader's no-database-file path is "a plain file path, resolved
            // relative to the working directory". The client is started from its install directory;
            // this build takes the directory the dats came from, which is the same place.
            shell.client_dir = self.cx.config().dat_dir.clone();
            shell.movie_bytes = H::movie_bytes;
            tracing::info!(
                "UI up, {} mode(s) registered, {:?} queued",
                dereth_ui::framework::mode::REGISTRATION_ORDER.len(),
                shell.flow.queued_mode()
            );
            self.front.ui = Some(shell);
            // The UI comes up on a pre-game screen, where no character session exists yet, so the
            // session's maps go now rather than on the UI's first frame: a key pressed before that
            // frame must not reach them either.
            if let Some(input) = self.shared.input.as_mut() {
                input.set_character_session_input_maps(false);
            }
        }
        Ok(())
    }

    /// The normal-render target callback, after camera/selection updates.
    /// Retail mutates the live regions before PresentFrame draws them. Final composition
    /// below observes these writes along with later input/network notice callbacks.
    fn draw_world_target(&mut self) {
        use dereth_ui_screens::hud::target::{self, VividTargetIndicator};
        use dereth_ui_screens::screens::gameplay_host::GameCall;
        let Some(shell) = self.front.ui.as_mut() else {
            return;
        };
        let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) else {
            return;
        };
        let GameCall::Root(Some(root)) =
            crate::hud_drive::game_call(&mut shell.ui, screen, GameCall::Root(None))
        else {
            return;
        };
        let world = self.cx.model();
        let mut state = VividTargetIndicator {
            enabled: self.cx.vivid_target_indicator(),
            display_on: dereth_client_runtime::hud::character_option(
                world,
                dereth_ui_screens::view::PlayerOption::VividTargetingIndicator,
            )
            .unwrap_or(false),
            target: None,
        };
        if let Some(id) = world.selected {
            if let Some(w) = world.weenie(id) {
                state.set_selected(
                    id,
                    world.player == Some(id),
                    world.is_owned_by_player(id),
                    w.current_state == dereth_client_model::weenie::PositionState::InContainer,
                );
            }
        }
        let viewport = self.cx.present().size();
        let projection = state.target.and_then(|id| {
            let (present, world) = self.cx.present_with_world();
            present.target_projection(id, world)
        });
        let color = dereth_ui_screens::mapradar::radar::get_blip_color(
            self.cx
                .hud()
                .radar
                .iter()
                .find(|entry| Some(entry.id) == state.target),
        );
        target::draw(
            &mut shell.ui,
            root,
            state,
            projection,
            0xFF00_0000 | color.hex,
            (viewport.0 as i32, viewport.1 as i32),
        );
    }

    /// End-of-frame drawing consumes the live UI after WorldObjects callbacks, not a snapshot taken
    /// during UI use time. Traverse once outside the device frame
    /// because newly referenced media can upload textures. No UI tick, input, or notices
    /// are repeated. Mode teardown still releases its old texture links in ui_use_time.
    fn compose_ui_draw_list(&mut self) {
        if let Some(shell) = self.front.ui.as_mut() {
            self.shared.ui_draw_list = shell.draw_list();
            let interface = std::sync::Arc::clone(&shell.interface);
            let world = std::sync::Arc::clone(self.cx.store());
            self.cx
                .present_mut()
                .prepare_ui(&interface, &world, &self.shared.ui_draw_list);
        }
    }

    /// Update cursor state from this build's five live inputs.
    ///
    /// Gathers the five inputs, runs [`crate::cursor::update_cursor_state`], and drains whatever
    /// the UI cursor setter lets through onto the window. Where each input comes from,
    /// and which of them have a writer today:
    ///
    /// | input | source | has a writer? |
    /// |---|---|---|
    /// | busy count | the world's busy count (`dereth_client_model::magic::MagicState::busy_count`) | yes |
    /// | target mode | [`dereth_client_runtime::interaction::Interaction::target_mode`] | yes, all four modes |
    /// | combat mode | `dereth_client_model::.combat_mode` | yes |
    /// | found object id | [`dereth_client_runtime::pick::WorldPicker::click_object`] | live mouse move / global loop: synchronous exact item-slot identity, or completed world-draw geometry pick |
    /// | target compatibility | [`crate::cursor::is_target_compatible_with_targeting_object`] | yes, |
    ///
    /// **The busy count** is one shared counter. A teleport (and the log-in's portal space, and a
    /// log-off's fade) raises it until the world fades back in; a cast, a use, a targeted use and
    /// a shop request raise it until the use-done acknowledgement; a swing the server commenced
    /// raises it until the attack is done; an examine raises it until its answer; and the
    /// allegiance panel's request raises it until the allegiance update answers it.
    fn update_cursor_state(&mut self) {
        let found = self.cx.found_object();
        let inputs = crate::cursor::CursorInputs {
            // The hourglass is up while anything the player asked for is still waiting.
            busy: self.cx.busy_count(),
            target_mode: self.cx.target_mode().into(),
            combat_mode: self.cx.model().combat.combat_mode,
            hovering: found.0 != 0,
            // Ask whether the found object is compatible with the targeting object.
            // The original item survives selection changes until target acquisition consumes it.
            target_compatible: crate::cursor::is_target_compatible_with_targeting_object(
                self.cx.model(),
                self.cx.model().targeting_object,
                found,
            ),
        };
        // `None` on a `--no-ui` run, which gates the
        // `SetCursor` and nothing else.
        // The pointers are the interface's own art.
        let interface = self.front.ui.as_ref().map_or_else(
            || self.cx.store().interface_files(),
            |s| std::sync::Arc::clone(&s.interface),
        );
        let mut shell = self.front.ui.as_mut();
        let ui = shell.as_mut().map(|s| &mut s.ui);
        self.shared
            .cursor
            .update_cursor_state(&*interface, ui, inputs);
        if let Some(s) = self.front.ui.as_mut() {
            self.shared.cursor.apply_pending(&interface, &mut s.ui);
        }
    }
}

/// The host clipboard, as the classic text fields reach it.
struct ClassicClipboard<'a, C: crate::clipboard::HostClipboard>(&'a mut C);

impl<C: crate::clipboard::HostClipboard> dereth_classic_ui::runtime::Clipboard
    for ClassicClipboard<'_, C>
{
    fn get(&mut self) -> Option<String> {
        self.0.get_text().ok().flatten()
    }
    fn set(&mut self, text: &str) {
        let _ = self.0.set_text(text);
    }
}

/// **The journal-path builder's three inputs.** The same three the screen-layout path is built
/// from, and the one place in the client that holds all of them: the preferences file is
/// `--prefs`, the world name comes from `0xF7E1 Login_WorldInfo` and the character is recorded on
/// the log-on edge by `run_character_actions`. Recomposed each frame rather than latched, because
/// the character changes with every log-on and the journal is per character. `None` while any of
/// the three is missing, which is a build that writes no journal at all rather than one that
/// writes to a guessed path.
fn journal_identity<H: Host>(
    cx: &Cx<'_, H>,
) -> Option<dereth_ui_screens::panels::journal::JournalIdentity> {
    let dir = cx.config().preferences_file.parent()?;
    let world = cx
        .pregame()
        .world_name
        .as_deref()
        .filter(|s| !s.is_empty())?;
    let character = cx
        .pregame()
        .entered_character
        .as_deref()
        .filter(|s| !s.is_empty())?;
    Some(dereth_ui_screens::panels::journal::JournalIdentity {
        directory: dir.to_path_buf(),
        world: world.to_owned(),
        character: character.to_owned(),
    })
}

fn flush_panel_sessions<H: Host>(cx: &mut Cx<'_, H>) {
    use dereth_client_contract::{book::BookAction, journal::JournalAction, UiRequest};
    let now = dereth_primitives::LocalTime(cx.now());
    if let Some(book) = cx.model().book.open.as_ref().map(|b| b.book_id) {
        let _ = cx.run_request(
            UiRequest::Book(BookAction::Flush { book }),
            now,
            &mut |_, _| false,
        );
    }
    let _ = cx.run_request(
        UiRequest::Journal(JournalAction::Visibility(false)),
        now,
        &mut |_, _| false,
    );
}

fn service_journal<H: Host>(cx: &mut Cx<'_, H>) {
    use dereth_client_contract::journal::JournalIo;
    use dereth_client_runtime::platform::files;
    use dereth_ui_screens::panels::journal::{
        parse_pages, save_pages_text, LOAD_COMPLAINT, LOAD_COMPLAINT_CHANNEL,
    };
    let identity = journal_identity(cx);
    cx.hud_mut().journal_identity = identity.clone();
    cx.prepare_journal(identity);
    for io in cx.take_journal_io() {
        match io {
            JournalIo::Load {
                identity,
                generation,
                revision,
            } => {
                let read = files::read_to_string(&identity.client_path());
                cx.record_journal_io(&identity, generation, true, read.is_ok());
                let pages = match read {
                    Ok(text) => parse_pages(&text),
                    Err(_) => Ok(Vec::new()),
                };
                if cx.complete_journal_load(identity, generation, revision, pages) {
                    cx.add_scroll_line(LOAD_COMPLAINT, LOAD_COMPLAINT_CHANNEL);
                }
            }
            JournalIo::Save {
                identity,
                generation,
                pages,
            } => {
                let path = identity.client_path();
                let saved = files::make_dirs(&identity.directory)
                    .and_then(|()| files::write(&path, save_pages_text(&pages)));
                cx.record_journal_io(&identity, generation, false, saved.is_ok());
                if let Err(error) = saved {
                    tracing::warn!(%error, "the journal could not be saved");
                }
            }
        }
    }
}

/// The classic key map as the classic interface reads it.
fn classic_keys(
    input: Option<&mut crate::input::InputShell>,
) -> dereth_classic_ui::keystore::ClassicKeys {
    input.map_or_else(Default::default, crate::input::InputShell::classic_keys)
}

/// The modern Load Keymap menu's first entry: the shipped maps, this interface's defaults.
pub const KEYMAP_DEFAULT: &str = "Default";

/// Bring the classic interface up: its art from the early-2005 portal, its text from the host's
/// fonts, its settings and keys in the stores both interfaces share.
fn build_classic<H: Host>(
    cx: &mut Cx<'_, H>,
    keys: dereth_classic_ui::keystore::ClassicKeys,
    creation: Result<std::rc::Rc<dereth_classic_ui::panels::pregame::data::CreationData>, String>,
) -> Result<dereth_classic_ui::runtime::ClassicUi, crate::classic_face::Refusal> {
    use crate::classic_face::Refusal;
    let portal = dereth_classic_dat::ClassicPortal::of_store(cx.store()).ok_or(Refusal::Files)?;
    let fonts = H::classic_fonts().ok_or(Refusal::Fonts)?;
    let art = dereth_classic_ui::art::ClassicArt::new(portal, &*fonts).map_err(Refusal::Failed)?;
    let art = std::sync::Arc::new(art);
    let help = H::classic_help_book()
        .map_err(Refusal::Failed)?
        .map(|bytes| dereth_classic_ui::help::decode(&bytes).map(std::sync::Arc::new))
        .transpose()
        .map_err(Refusal::Failed)?;
    let resources = dereth_classic_ui::resources::Resources::new(art, creation, help);
    let cfg = cx.config();
    let state = cfg
        .preferences_file
        .parent()
        .map_or_else(
            || std::path::PathBuf::from("."),
            std::path::Path::to_path_buf,
        )
        .join("classic");
    let size = cx.present().size();
    let mut ui = dereth_classic_ui::runtime::ClassicUi::new(
        resources,
        dereth_classic_ui::art::ClassicPaths { state },
        dereth_classic_ui::panels::factory,
        size,
    );
    ui.set_classic_keys(keys);
    ui.classic.welcome = H::classic_welcome();
    ui.start(cx).map_err(Refusal::Failed)?;
    Ok(ui)
}

impl<H: Host> ClientShell<H> {
    /// After the data files are reopened (a data patch), both interfaces read them again, the
    /// one not shown too, so whichever is shown next draws the new records.
    fn reread_files_when_patched(&mut self, cx: &mut Cx<'_, H>) {
        let now = cx.store_generation();
        if now == self.shared.files_read {
            return;
        }
        self.shared.files_read = now;
        <ModernFrontEnd as FrontEnd<H>>::reread_files(&mut self.modern, cx, &mut self.shared);
        if let Some(ui) = self.classic.ui.as_mut() {
            <dereth_classic_ui::runtime::ClassicUi as FrontEnd<H>>::reread_files(
                ui,
                cx,
                &mut self.shared,
            );
        }
        if let Some(ui) = self.horizon.ui.as_mut() {
            <dereth_horizon::runtime::HorizonFrontEnd as FrontEnd<H>>::reread_files(
                ui,
                cx,
                &mut self.shared,
            );
            ui.set_chargen_tables(
                self.modern
                    .ui
                    .as_ref()
                    .and_then(crate::ui::UiShell::chargen_tables),
            );
        }
    }

    fn front(&self) -> &dyn FrontEnd<H> {
        if let Some(ui) = self.classic.active() {
            return ui;
        }
        match self.horizon.active() {
            Some(ui) => ui,
            None => &self.modern,
        }
    }
    fn front_and_services(&mut self) -> (&mut dyn FrontEnd<H>, &mut FrontEndServices<H>) {
        if let Some(ui) = self.classic.active_mut() {
            return (ui, &mut self.shared);
        }
        match self.horizon.active_mut() {
            Some(ui) => (ui, &mut self.shared),
            None => (&mut self.modern, &mut self.shared),
        }
    }

    fn sync_classic_input(
        &mut self,
        cx: &mut Cx<'_, H>,
        capturing: bool,
        key: Option<u16>,
        host_message: bool,
    ) {
        let Some(ui) = self.classic.active() else {
            return;
        };
        let (editing, modal, own_capture) = ui.input_scope(None);
        let barrier = ui.keyboard_barrier(key);
        let recall = ui.chat_focused() && !barrier;
        let Some(input) = self.shared.input.as_mut() else {
            return;
        };
        if let Some(ui) = self.classic.active_mut() {
            input.defer_runtime_actions(ui.take_runtime_actions());
        }
        input.set_character_session_input_maps(cx.pregame().in_world);
        input.finish_session_retirement(|released| {
            if host_message {
                for action in released {
                    cx.inject_action(action);
                }
            } else {
                cx.accept_actions(released);
            }
        });
        input.set_focused_input_maps(if editing {
            &crate::input::FOCUSED_TEXT_MAP_REGISTRATIONS
        } else if barrier {
            &[(1, 2990)]
        } else {
            &[]
        });
        input.classic_recall_scope(recall);
        input.set_mode_input_maps(if modal || !cx.pregame().in_world {
            &[9]
        } else {
            &[]
        });
        if input.manager.text.text_mode != editing {
            input.set_text_mode(editing);
        }
        input.set_key_hit_handler(capturing || own_capture);
    }

    fn drain_classic_message(&mut self, cx: &mut Cx<'_, H>) -> bool {
        let Some(input) = self.shared.input.as_mut() else {
            return false;
        };
        let (events, chars) = input.drain_message();
        let consumed = events
            .iter()
            .any(|event| !matches!(event.input_map.0, 1 | 7 | 8 | 9 | 10));
        input.take_key_hits();
        for event in events {
            input.manager.begin_action_dispatch(event.from_key_down);
            if let Some(ui) = self.classic.active_mut() {
                ui.mapped_action(cx, event);
                let editing = ui.input_scope(None).0;
                if input.manager.text.text_mode != editing {
                    input.set_text_mode(editing);
                }
            }
            input.manager.end_action_dispatch();
        }
        if let Some(ui) = self.classic.active_mut() {
            ui.mapped_characters(cx, chars);
            input.defer_runtime_actions(ui.take_runtime_actions());
        }
        consumed
    }

    /// The Horizon interface's share of the input manager's actions this frame: its own keys and the
    /// mouse actions over the world; the rest go on to the game. Then where the objects whose names
    /// it shows are on screen, for its frame.
    fn horizon_input(&mut self, cx: &mut Cx<'_, H>, now: dereth_primitives::LocalTime) {
        let Some(ui) = self.horizon.ui.as_mut() else {
            return;
        };
        if let Some(code) = cx.hud_mut().horizon.abuse.take() {
            ui.abuse_response(code);
        }
        if let Some(input) = self.shared.input.as_mut() {
            // The key bindings page: what it asked last frame, the key it waits for, and the
            // bindings as they are now.
            self.horizon
                .keys
                .serve(input, std::mem::take(&mut ui.key_requests));
            for text in self.horizon.keys.lines.drain(..) {
                ui.outbox
                    .emit(dereth_client_contract::UiRequest::DisplayChatText {
                        feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                        channel: dereth_ui_screens::options::config::MOUSE_TURNING_CHANNEL,
                        text: text.to_owned(),
                    });
            }
            if let Some(view) = self.horizon.keys.view(input, cx.store()) {
                ui.keys = Some(view);
            }
            // The input maps this interface's state calls for, as the other interfaces register
            // them: the character session's (movement, the panels, the shortcuts, combat, chat)
            // while its game screen is up, the pre-game screen's otherwise, the text box's while
            // one has the keyboard, and the target map while a use waits for its target.
            let game = ui.in_gameplay();
            input.set_character_session_input_maps(game);
            input.set_mode_input_maps(if game { &[] } else { &[9] });
            input.set_focused_input_maps(if ui.ui.text_focus {
                &crate::input::FOCUSED_TEXT_MAP_REGISTRATIONS
            } else {
                &[]
            });
            input.set_target_input_map(
                cx.target_mode() != dereth_client_runtime::interaction::TargetMode::None,
            );
            input.finish_session_retirement(|released| cx.accept_actions(released));
            if input.manager.text.text_mode != ui.ui.text_focus {
                input.set_text_mode(ui.ui.text_focus);
            }
            input.use_time(now);
            let events = input.take_events();
            let rest = ui.take_actions(cx, events, input.mouse_pos(), now);
            input.put_back_unconsumed(rest);
        }
        let wanted = std::mem::take(&mut ui.wanted_projections);
        let (present, world) = cx.present_with_world();
        ui.origins = wanted
            .iter()
            .filter_map(|&id| Some((id, present.target_origin(id, world)?)))
            .collect();
        ui.tops = wanted
            .iter()
            .filter_map(|&id| Some((id, present.target_top(id, world)?)))
            .collect();
        ui.projections = wanted
            .into_iter()
            .filter_map(|id| Some((id, present.target_projection(id, world)?)))
            .collect();
        ui.in_sight = present.drawn_objects();
    }

    /// The interface shown now.
    fn shown_interface(&self) -> dereth_client_contract::options::interface::Interface {
        use dereth_client_contract::options::interface::Interface;
        if self.classic.active {
            Interface::Classic
        } else if self.horizon.active {
            Interface::Horizon
        } else {
            Interface::Modern
        }
    }

    /// Follow the interface choice: bring the chosen interface up if it is not up yet, put the
    /// one shown away and show the chosen one; a choice that cannot be shown goes back to the one
    /// shown, and the chat says why. A choice of Horizon while its art is still loading waits:
    /// the one shown stays, the chat says so once, and the choice is followed once the art is in.
    fn follow_interface(&mut self, cx: &mut Cx<'_, H>) {
        use dereth_client_contract::options::interface::Interface;
        let shown = self.shown_interface();
        // The lines a chat window was handed since the last frame, each once: a line still
        // waiting to be delivered is recorded when it is, however many frames it waits.
        let delivered = std::mem::take(&mut cx.hud_mut().delivered_chat);
        self.classic
            .remember(&delivered, shown == Interface::Modern);
        let Some(want) = self.classic.changed_choice() else {
            return;
        };
        if want == shown {
            self.horizon.told_loading = false;
            return;
        }
        if let Err(refusal) = self.bring_up(cx, want) {
            if refusal == crate::classic_face::Refusal::HorizonArtLoading {
                if !self.horizon.told_loading {
                    tracing::info!("{}", refusal.notice());
                    cx.add_scroll_line(
                        &refusal.notice(),
                        dereth_client_model::scroll::LOCAL_ERROR_TYPE,
                    );
                    self.horizon.told_loading = true;
                }
                self.classic.wait();
                return;
            }
            self.horizon.told_loading = false;
            tracing::warn!("{}", refusal.notice());
            cx.add_scroll_line(
                &refusal.notice(),
                dereth_client_model::scroll::LOCAL_ERROR_TYPE,
            );
            self.classic.refused(shown);
            return;
        }
        self.horizon.told_loading = false;
        match shown {
            Interface::Modern => self.leave_modern(cx),
            Interface::Classic => self.leave_classic(cx),
            Interface::Horizon => self.leave_horizon(cx),
        }
        match want {
            Interface::Modern => self.enter_modern(cx),
            Interface::Classic => self.enter_classic(cx),
            Interface::Horizon => self.enter_horizon(cx),
        }
        // The interface shown now never saw the press that turned the camera with the pointer:
        // the camera stops turning with it, and a pointer held for the drag is shown again where
        // the drag began.
        if cx.mouse_look() {
            cx.mouse_look_button(false);
        }
        self.shared.pointer.let_go();
    }

    /// Bring `want` up, if it is not up already.
    fn bring_up(
        &mut self,
        cx: &mut Cx<'_, H>,
        want: dereth_client_contract::options::interface::Interface,
    ) -> Result<(), crate::classic_face::Refusal> {
        use dereth_client_contract::options::interface::Interface;
        match want {
            Interface::Modern => {}
            Interface::Classic => {
                if let Some(ui) = self.classic.ui.as_mut() {
                    ui.shown_again();
                    ui.set_classic_keys(classic_keys(self.shared.input.as_mut()));
                } else {
                    let creation = self.modern.ui.as_ref().map_or_else(
                        || Err("World creation tables unavailable".to_owned()),
                        crate::ui::UiShell::classic_creation_data,
                    );
                    self.classic.ui = Some(build_classic(
                        cx,
                        classic_keys(self.shared.input.as_mut()),
                        creation,
                    )?);
                }
            }
            Interface::Horizon => {
                if let Some(ui) = self.horizon.ui.as_mut() {
                    ui.shown_again();
                } else {
                    self.horizon.ui = Some(horizon::build_horizon(cx)?);
                }
                // Creation and the barber read the creation tables the screens read.
                let tables = self
                    .modern
                    .ui
                    .as_ref()
                    .and_then(crate::ui::UiShell::chargen_tables);
                if let Some(ui) = self.horizon.ui.as_mut() {
                    ui.set_chargen_tables(tables);
                }
            }
        }
        Ok(())
    }

    /// Put the modern interface away: its widget drafts are captured and its sessions flushed,
    /// so the incoming interface projects the shared sessions as they stand.
    fn leave_modern(&mut self, cx: &mut Cx<'_, H>) {
        if let Some(shell) = self.modern.ui.as_mut() {
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    screen,
                    dereth_ui_screens::screens::gameplay_host::GameCall::CaptureBookDraft,
                );
            }
            cx.hud_mut().panels.journal.save_this_page(&mut shell.ui);
            let now = dereth_primitives::LocalTime(cx.now());
            for request in shell.ui.requests.take() {
                if matches!(
                    &request,
                    dereth_client_contract::UiRequest::Book(_)
                        | dereth_client_contract::UiRequest::Journal(_)
                ) {
                    let _ = cx.run_request(request, now, &mut |_, _| false);
                } else {
                    shell.ui.requests.emit(request);
                }
            }
        }
        flush_panel_sessions(cx);
        service_journal(cx);
        <ModernFrontEnd as FrontEnd<H>>::suspend(&mut self.modern, cx);
        dereth_client_contract::panels::HudPanels::spew_clear_pending(&mut cx.hud_mut().panels);
        if let Some(input) = self.shared.input.as_mut() {
            cx.accept_actions(input.release_actions());
        }
    }

    /// Show the modern interface again: it comes up on the screen the game is at, with the
    /// options and the chat drafts as they are now and the lines it missed.
    fn enter_modern(&mut self, cx: &mut Cx<'_, H>) {
        if let Some(input) = self.shared.input.as_mut() {
            if let Some(ui) = self.modern.ui.as_mut() {
                ui.resume_input(input);
            }
        }
        // This interface was not framed while another was shown: it comes up on the screen the
        // game is at, whatever it last showed.
        if let Some(shell) = self.modern.ui.as_mut() {
            shell.catch_up(cx.pregame());
        }
        // Another interface may have dressed the shared preview spaces with its own models: this
        // one builds its own again.
        self.modern.paper_doll_built = None;
        self.modern.preview_chargen = None;
        flush_panel_sessions(cx);
        service_journal(cx);
        // Discard widget caches; the shared notebook remains loaded.
        cx.hud_mut().panels.journal.forget();
        // The other interface may have changed any option while this one was put away (the
        // interface choice itself among them): every option page shows the store and the
        // character as they are now.
        if let Some(shell) = self.modern.ui.as_mut() {
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                let (hud, objects) = cx.hud_and_objects();
                let moved = hud.reread_option_pages(&mut shell.ui, screen, objects);
                tracing::debug!("the option pages read again: {moved} rows moved");
            }
        }
        if let Some(shell) = self.modern.ui.as_mut() {
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                for draft in cx.chat_entry_drafts() {
                    crate::hud_drive::game_call(
                        &mut shell.ui,
                        screen,
                        dereth_ui_screens::screens::gameplay_host::GameCall::ChatEntry(draft),
                    );
                }
            }
        }
        // This interface's chat takes the lines it missed.
        let missed: Vec<_> = self.classic.take_missed();
        cx.hud_mut().replayed_chat.extend(missed);
        tracing::info!("the modern interface is shown");
    }

    /// Put the classic interface away, and the two shared preferences it set live for itself
    /// alone (its own field of view, and the camera's inversion off while it inverts the vertical
    /// itself) back to the shared store's values.
    fn leave_classic(&mut self, cx: &mut Cx<'_, H>) {
        if let Some(ui) = self.classic.ui.as_mut() {
            ui.suspend(cx);
        }
        dereth_client_contract::panels::HudPanels::spew_clear_pending(&mut cx.hud_mut().classic);
        if let Some(input) = self.shared.input.as_mut() {
            cx.accept_actions(input.release_actions());
            input.activate_classic(false);
        }
        self.classic.active = false;
        cx.hud_mut().classic_active = false;
        flush_panel_sessions(cx);
        service_journal(cx);
        cx.apply_interface_overrides(dereth_client_runtime::ui_context::InterfaceOverrides::Modern);
    }

    /// Show the classic interface, with the chat history and drafts the game holds.
    fn enter_classic(&mut self, cx: &mut Cx<'_, H>) {
        if let Some(input) = self.shared.input.as_mut() {
            input.activate_classic(true);
        }
        self.classic.active = true;
        cx.hud_mut().classic_active = true;
        cx.apply_interface_overrides(
            dereth_client_runtime::ui_context::InterfaceOverrides::ClassicInput,
        );
        let size = cx.present().size();
        let history: Vec<_> = self.classic.history().cloned().collect();
        if let Some(ui) = self.classic.ui.as_mut() {
            ui.set_display((
                i32::try_from(size.0).unwrap_or(i32::MAX),
                i32::try_from(size.1).unwrap_or(i32::MAX),
            ));
            ui.classic.chat.clear();
            for line in history {
                ui.chat_line(
                    u32::from(line.ty),
                    line.prefix.unwrap_or_default(),
                    &line.body,
                    line.window,
                );
            }
            for draft in cx.chat_entry_drafts() {
                ui.apply_chat_entry(cx, draft);
            }
        }
        tracing::info!("the classic interface is shown");
    }

    /// Put the Horizon interface away: every press it holds is let go.
    fn leave_horizon(&mut self, cx: &mut Cx<'_, H>) {
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.suspend();
        }
        if let Some(input) = self.shared.input.as_mut() {
            cx.accept_actions(input.release_actions());
            input.set_text_mode(false);
            input.activate_horizon(false);
        }
        // The game's camera again, where it was left, and the bodies drawn as the game draws them.
        cx.set_orbit_camera(None);
        cx.set_smooth_animation(false);
        cx.set_smooth_movement(false);
        // The game's attack controls again: held, they charge.
        cx.set_press_attacks(false);
        cx.set_hifi_interface(false);
        self.horizon.active = false;
        cx.hud_mut().horizon_active = false;
        flush_panel_sessions(cx);
        service_journal(cx);
    }

    /// Show the Horizon interface, with the chat history the game holds.
    fn enter_horizon(&mut self, cx: &mut Cx<'_, H>) {
        // This interface's own keys, and its own camera.
        if let Some(input) = self.shared.input.as_mut() {
            input.activate_horizon(true);
        }
        if let Some(ui) = self.horizon.ui.as_ref() {
            cx.set_orbit_camera(Some(ui.ui.options.orbit));
        }
        self.horizon.active = true;
        cx.set_hifi_interface(true);
        // Another interface may have rebound keys while this one was put away.
        self.horizon.keys.invalidate();
        cx.hud_mut().horizon_active = true;
        let history: Vec<_> = self.classic.history().cloned().collect();
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.chat_history(history);
            // Another interface may have changed any option while this one was put away.
            ui.reread_option_rows(cx);
        }
        tracing::info!("the Horizon interface is shown");
    }

    /// One of the window's events, routed through the interface shown.
    fn route_window_event(
        &mut self,
        cx: &mut Cx<'_, H>,
        event: &dereth_input::host::HostEvent,
        time_ms: u32,
    ) {
        if self.horizon.active {
            route_horizon_event(cx, self, event, time_ms);
        } else if self.classic.active {
            self.route_classic_event(cx, event, time_ms);
        } else {
            route_host_event(cx, self, event, time_ms);
        }
    }

    /// One of the window's events while the classic interface is shown: a lifecycle event as any
    /// interface routes it, and a device event to the interface's widgets and its key map, in the
    /// order its own window procedure takes them.
    fn route_classic_event(
        &mut self,
        cx: &mut Cx<'_, H>,
        event: &dereth_input::host::HostEvent,
        time_ms: u32,
    ) {
        use dereth_input::host::HostEvent;
        if let Some(lifecycle) = crate::platform::window::lifecycle(event) {
            cx.window_event(self, &lifecycle, time_ms);
            if let Some(input) = self.shared.input.as_mut() {
                for message in dereth_client_runtime::pump::Pump::map_window_event(&lifecycle) {
                    input.on_message(crate::pump::from_window(message, time_ms));
                }
            }
        }
        let key = match event {
            HostEvent::KeyboardInput { key, .. } => u16::try_from(key.virtual_key).ok(),
            _ => None,
        };
        if let Some(ui) = self.classic.active_mut() {
            ui.prepare_host_input(cx, event);
        }
        let capturing = self
            .classic
            .active()
            .is_some_and(|ui| ui.input_scope(key).2);
        self.sync_classic_input(cx, capturing, key, true);
        // The widget receives key transitions, but characters come only from the
        // normalized input stream after its text-mode gate.
        let widget_event = match event {
            HostEvent::KeyboardInput { key, pressed, .. } => HostEvent::KeyboardInput {
                key: *key,
                pressed: *pressed,
                text: None,
            },
            _ => event.clone(),
        };
        let keyboard = matches!(event, HostEvent::KeyboardInput { .. });
        if !keyboard {
            if let Some(ui) = self.classic.active_mut() {
                ui.window_input(
                    cx,
                    std::slice::from_ref(&widget_event),
                    &mut ClassicClipboard(&mut self.shared.host_clipboard),
                );
            }
        }
        let mut widget_key_pending = keyboard;
        for message in self.shared.devices.map_device_event(event, time_ms) {
            cx.window_message(crate::pump::window_message(message), message.time_ms);
            if let Some(input) = self.shared.input.as_mut() {
                input.on_message(message);
            }
            let consumed = self.drain_classic_message(cx);
            if widget_key_pending {
                widget_key_pending = false;
                if !consumed || capturing {
                    if let Some(ui) = self.classic.active_mut() {
                        ui.window_input(
                            cx,
                            std::slice::from_ref(&widget_event),
                            &mut ClassicClipboard(&mut self.shared.host_clipboard),
                        );
                    }
                }
                self.sync_classic_input(cx, capturing, key, true);
            }
        }
        self.drain_classic_message(cx);
        self.sync_classic_input(cx, false, None, true);
    }

    /// The camera drag after one of the window's events: the pointer is held once the drag has
    /// moved it far enough, and shown again where the drag began when the camera stops turning
    /// with it, which the interface shown is told as the pointer moving there. Gamepad mode holds
    /// nothing.
    fn follow_pointer_drag(&mut self, cx: &mut Cx<'_, H>, time_ms: u32) {
        let gamepad_mode = self
            .horizon
            .active()
            .is_some_and(|ui| ui.ui.options.pad.enabled);
        self.shared.pointer.follow(cx.mouse_look(), !gamepad_mode);
        if let Some((x, y)) = self.shared.pointer.take_returned() {
            let back = dereth_input::host::HostEvent::CursorMoved { x, y };
            self.route_window_event(cx, &back, time_ms);
        }
    }

    /// The mouse moved while the pointer is held, and the camera's pointer is now at `(x, y)`:
    /// the camera turns as it does for the pointer's own movement, and the interface's pointer
    /// stays where it was held.
    fn camera_pointer(&mut self, cx: &mut Cx<'_, H>, x: f64, y: f64) {
        match self.classic.active_mut() {
            Some(ui) => ui.camera_pointer(cx, x, y),
            None => cx.cursor_moved(x, y),
        }
    }
}

impl<H: Host> Shell for ClientShell<H> {
    type Hud = crate::hud::Hud;
    type Present = dyn ClientPresentation;

    /// The keymap first: the user's `.keymap` is merged **first** so a rebound key wins over both
    /// shipped defaults, and the merged map is written back on exit. Not fatal: a client with no
    /// key bindings is useless but can still draw, and saying so is better than refusing to start.
    fn start_input(&mut self, cx: &mut Cx<'_, H>) {
        let preferences_file = cx.config().preferences_file.clone();
        let keymap_file = dereth_client_runtime::platform::files::read_to_string(&preferences_file)
            .ok()
            .and_then(|text| {
                dereth_client_contract::persist::preferences::UserPreferences::parse(&text).ok()
            })
            .and_then(|prefs| {
                prefs
                    .get(dereth_client_contract::persist::preferences::keys::KEYMAP_FILE)
                    .map(str::to_owned)
            });
        let keymap = crate::input::keymap_path_for(&preferences_file, keymap_file.as_deref());
        match crate::input::InputShell::new(&**cx.store(), keymap.as_ref()) {
            Ok(i) => self.shared.input = Some(i),
            Err(e) => tracing::warn!("no input: {e}"),
        }
    }

    /// The window's queued events, in arrival order, then the end of the drain.
    ///
    /// Each event passes the pointer's hold first ([`crate::pointer`]): while a camera drag holds
    /// the pointer, the mouse's own movement turns the camera alone and the pointer's reports are
    /// not routed, and when the drag ends the interface is told the pointer is back where the
    /// drag began. A drag is never a click: before a button's release after a drag is routed,
    /// the device input forgets the press, so the next is no second click of a double-click, and
    /// a right button's release is said to end a drag, so it examines nothing.
    fn window_input(&mut self, cx: &mut Cx<'_, H>, time_ms: u32) {
        cx.set_chat_interface(self.shown_interface());
        let events: Vec<_> = self.shared.window_events.borrow_mut().drain(..).collect();
        // The pad is read every frame, so the host keeps up with it whichever interface is shown;
        // only the Horizon interface answers it, after the window's events, so in gamepad mode the
        // pad's pointer is the one the interface sees.
        let pad = self.shared.gamepad.poll();
        let pad = self.shared.scripted_pad.or(pad);
        // A drag that ended after the last frame's events (the interface changed, say).
        self.follow_pointer_drag(cx, time_ms);
        for event in &events {
            let gate = self.shared.pointer.gate(event);
            if let Some((button, dragged)) = self.shared.pointer.take_release() {
                if dragged {
                    if let Some(input) = self.shared.input.as_mut() {
                        input.manager.forget_mouse_press(button);
                    }
                }
                if button == dereth_input::keys::MouseButton::Right {
                    cx.note_right_release(dragged);
                }
            }
            match gate {
                crate::pointer::Gate::Route => self.route_window_event(cx, event, time_ms),
                crate::pointer::Gate::Camera(x, y) => self.camera_pointer(cx, x, y),
                crate::pointer::Gate::Drop => {}
            }
            self.follow_pointer_drag(cx, time_ms);
        }
        if self.horizon.active {
            let keys = self
                .horizon
                .active_mut()
                .map(|ui| ui.pad_input(cx, pad))
                .unwrap_or_default();
            for event in &keys {
                route_horizon_event(cx, self, event, time_ms);
            }
        }
    }

    fn input_use_time(&mut self, _cx: &mut Cx<'_, H>, now: dereth_primitives::LocalTime) {
        if let Some(input) = self.shared.input.as_mut() {
            input.use_time(now);
        }
    }

    fn hand_on_actions(&mut self, actions: &mut dereth_client_runtime::actions::ActionQueue) {
        if let Some(ui) = self.classic.active_mut() {
            ui.hand_on_actions(actions);
        }
        if let Some(input) = self.shared.input.as_mut() {
            input.hand_on(actions);
        }
    }

    fn control_notice(&mut self, notice: dereth_client_runtime::shell::ControlNotice) {
        if let Some(input) = self.shared.input.as_mut() {
            input.apply_notice(notice);
        }
    }

    /// The key map, when it was loaded and its configured filename is non-empty: keymap
    /// serialization writes the **full merged map**, defaults included, and the next run merges it
    /// back first so it wins. `Nothing` is the client's own skip when that filename is empty.
    fn save_bindings(&mut self) -> dereth_client_runtime::shutdown::Outcome {
        use dereth_client_runtime::shutdown::Outcome;
        if let Some(input) = self.shared.input.as_mut() {
            if let Err(e) = input.save_horizon_keymap() {
                tracing::warn!("the Horizon key map was not saved: {e}");
            }
        }
        match self
            .shared
            .input
            .as_ref()
            .map(crate::input::InputShell::save_keymap)
        {
            Some(Ok(true)) => Outcome::Ran,
            Some(Ok(false)) | None => Outcome::Nothing,
            Some(Err(e)) => {
                tracing::warn!("the keymap was not saved: {e}");
                Outcome::Nothing
            }
        }
    }

    fn start_ui(&mut self, cx: &mut Cx<'_, H>) -> Result<(), StartupError> {
        Ui {
            cx,
            front: &mut self.modern,
            shared: &mut self.shared,
        }
        .start_ui()
    }

    fn has_ui(&self) -> bool {
        self.modern.ui.is_some()
            || self.classic.active().is_some()
            || self.horizon.active().is_some()
    }

    /// Whether the current UI mode is the gameplay screen, which means "the player is in the world".
    fn in_gameplay(&self) -> bool {
        self.front().in_gameplay()
    }

    /// The Horizon interface draws its screens before the world at the player's own size.
    fn keeps_login_size(&self) -> bool {
        !self.horizon.active
    }

    /// Credits has no backdrop element. Retail's black is the frame-start clear:
    /// the character-management screen builds the rotating preview, while the credits screen
    /// only creates its two authored roots, so entering Credits hides the retained world.
    fn hides_world(&self) -> bool {
        self.front().hides_world()
    }

    /// A Turbine callback is a synchronous notice to the CURRENT chat subscribers. Preserve that
    /// generation through the deferred Hud delivery, not across a rebuild.
    fn chat_generation(&self) -> Option<u64> {
        self.front().chat_generation()
    }

    fn clear_chat_history(&mut self) {
        self.classic.clear_history();
        if let Some(ui) = self.horizon.ui.as_mut() {
            ui.clear_chat();
        }
    }

    fn ui_requests(&mut self) -> Option<&mut dereth_client_contract::requests::Outbox> {
        self.front_and_services().0.requests()
    }

    /// The 3D viewport's rectangle, from `<SBOX>`'s own screen box.
    ///
    /// The viewport handler consumes four values: the element's screen x0, screen y0, width, and
    /// height. `dereth_ui`'s boxes are **inclusive**, hence the `+ 1`s -- the same conversion the
    /// portal space's own rect does one screen over.
    ///
    /// `None` when there is no shell, no gameplay screen, or no `<SBOX>` in it: a `--no-ui` run
    /// and every pre-gameplay screen, where the scene owns the whole back buffer. That is also
    /// the original viewport calculation's answer with nothing docked, so the two agree
    /// on the degenerate case rather than merely not disagreeing.
    fn game_viewport(&self) -> Option<dereth_primitives::Viewport> {
        self.front().game_viewport()
    }

    /// Is the pointer over the 3-D view, rather than over a HUD window drawn on
    /// top of it? Retail asks with a rectangle that degenerates to the whole window in the
    /// shipped layout, so this build asks the hit test instead -- the same machinery the click
    /// path consults. `true` with no UI at all.
    fn pointer_over_game_view(&self, cursor: (i32, i32)) -> bool {
        self.front().pointer_over_game_view(cursor)
    }

    /// Look up element `0x100005F7` and read its visibility bit: is `<EXAM>` on
    /// screen? `false` covers both negative legs: no UI manager and no such element.
    fn examine_panel_open(&mut self) -> bool {
        self.front_and_services().0.examine_panel_open()
    }

    /// Use the panel's own hide path so the key and close button
    /// reach one statement and one counter.
    fn close_examine_panel(&mut self) {
        self.front_and_services().0.close_examine_panel()
    }

    fn service_dialogs(&mut self, cx: &mut Cx<'_, H>, now: dereth_primitives::LocalTime) {
        self.front_and_services().0.service_dialogs(cx, now)
    }

    fn resolution_prompt(
        &mut self,
        cx: &mut Cx<'_, H>,
        prompt: Option<dereth_client_contract::resolution::ResolutionPrompt>,
    ) {
        if let Some(classic) = self.classic.active_mut() {
            if let Some(ui) = self.modern.ui.as_mut() {
                self.modern.resolution_dialog.clear(&mut ui.ui);
            }
            classic.project_resolution(prompt);
        } else if let Some(horizon) = self.horizon.active_mut() {
            if let Some(ui) = self.modern.ui.as_mut() {
                self.modern.resolution_dialog.clear(&mut ui.ui);
            }
            horizon.project_resolution(prompt);
        } else if let Some(ui) = self.modern.ui.as_mut() {
            ui.ui.now = dereth_primitives::LocalTime(cx.now());
            self.modern.resolution_dialog.project(&mut ui.ui, prompt);
        }
    }

    fn before_ui_input(&mut self, player_airborne: bool) {
        self.front_and_services().0.before_ui_input(player_airborne)
    }

    fn drive_pregame_screens(&mut self, cx: &mut Cx<'_, H>) {
        self.scripted_pregame(cx);
    }

    fn drive_world_script(&mut self, cx: &mut Cx<'_, H>, now: dereth_primitives::LocalTime) {
        self.scripted_world(cx, now);
    }

    /// Where the portal space draws: the whole world-controller UI viewport region. The portal-space
    /// element is element `0x10000436`, an 800x600 viewport element under `0x10000037` in layout
    /// `0x2100000F`.
    fn place_portal_space(&mut self, present: &mut Self::Present) {
        if let Some(ui) = self.classic.active_mut() {
            ui.place_portal_space(present.size());
            return;
        }
        if let Some(ui) = self.horizon.active_mut() {
            // The swirl goes under the whole interface, over the whole window.
            ui.portal = true;
            return;
        }
        let id = crate::gpu::PreviewId::Portal;
        let who = self.modern.ui.as_ref().and_then(|shell| {
            let root = *shell.flow.current()?.roots().first()?;
            let h = shell
                .ui
                .get_child_recursive(root, dereth_ui_screens::hud::world_view::PORTAL_SPACE)?;
            let b = shell.ui.screen_box(h);
            b.is_valid().then_some((h, b))
        });
        let (w, h) = present.size();
        match who {
            Some((handle, area)) => {
                #[allow(clippy::cast_sign_loss)]
                // LINT-OK: `is_valid` above proves the box is non-empty; `max(0)` covers an
                // element laid out off the top-left edge. The box is inclusive, hence the +1.
                let rect = dereth_render::camera::Viewport {
                    x: area.x0.max(0) as u32,
                    y: area.y0.max(0) as u32,
                    width: (area.x1 - area.x0.max(0) + 1).max(0) as u32,
                    height: (area.y1 - area.y0.max(0) + 1).max(0) as u32,
                };
                present.preview_queue(id, handle, rect);
            }
            // No shell at all (a `--no-ui` run), or a shell whose tree has no smart box: the
            // space is the whole window, which is what the element's own box is anyway. The
            // handle names no live element, so `draw_ui`'s fallback pass is what runs it.
            None => present.preview_queue(
                id,
                dereth_ui::ElemHandle::from_raw(u32::MAX),
                dereth_render::camera::Viewport {
                    x: 0,
                    y: 0,
                    width: w,
                    height: h,
                },
            ),
        }
    }

    /// The screens carry `--enter-world` whenever they are up; with no UI the runtime does.
    fn drives_scripted_entry(&self) -> bool {
        self.modern.ui.is_some() && !self.classic.active && !self.horizon.active
    }

    fn ui_frame(
        &mut self,
        cx: &mut Cx<'_, H>,
        now: dereth_primitives::LocalTime,
        notices: UiNotices,
    ) {
        service_journal(cx);
        self.reread_files_when_patched(cx);
        self.follow_interface(cx);
        cx.set_chat_interface(self.shown_interface());
        if self.classic.active {
            self.sync_classic_input(cx, false, None, false);
            if let Some(input) = self.shared.input.as_mut() {
                input.use_time(now);
                let events = input.take_events();
                if let Some(ui) = self.classic.active_mut() {
                    for event in events {
                        ui.mapped_action(cx, event);
                    }
                }
            }
        }
        if self.horizon.active {
            self.horizon_input(cx, now);
        }
        let changed = {
            let (front, services) = self.front_and_services();
            front.frame(cx, services, now, notices)
        };
        if let Some(input) = self.shared.input.as_mut() {
            input.finish_session_retirement(|released| cx.accept_actions(released));
        }
        if changed {
            cx.follow_screen_change(self);
        }
        {
            let (front, services) = self.front_and_services();
            front.before_portal(cx, services);
        }
        if !self.classic.active {
            cx.teleport_use_time(self);
            cx.portal_space_use_time(self);
        }
        {
            let (front, services) = self.front_and_services();
            front.after_portal(cx, services);
        }
        service_journal(cx);
    }

    fn talk_focus_notice(
        &mut self,
        talk_focus: dereth_client_model::chat::TalkFocus,
        notice: dereth_client_model::chat::TalkFocusNotice,
    ) -> bool {
        self.front_and_services()
            .0
            .talk_focus_notice(talk_focus, notice)
    }

    /// Jump's notices are synchronous with their input/control-loss owner. Finish the
    /// existing subscriber before the next action, not next frame. An absent or outgoing
    /// subscriber leaves no retained notice history.
    fn deliver_power_bar_notices(
        &mut self,
        hud: &mut crate::hud::Hud,
        notices: Vec<dereth_client_model::combat::PowerBarNotice>,
    ) {
        self.front_and_services()
            .0
            .deliver_power_bar_notices(hud, notices)
    }

    fn object_panel_notice(
        &mut self,
        hud: &mut crate::hud::Hud,
        world: &dereth_client_model::World,
        notice: &dereth_client_model::Notice,
    ) -> Vec<dereth_client_contract::UiRequest> {
        self.front_and_services()
            .0
            .object_panel_notice(hud, world, notice)
    }

    fn emit_magic_notices(&mut self, notices: Vec<dereth_client_contract::view::MagicNotice>) {
        self.front_and_services().0.emit_magic_notices(notices)
    }

    fn open_vendor_buying(&mut self, hud: &mut crate::hud::Hud) {
        self.front_and_services().0.open_vendor_buying(hud)
    }

    fn run_ui_layout_commands(
        &mut self,
        prefs: &std::path::Path,
        character: &str,
        world: &str,
        layout_commands: Vec<dereth_client_runtime::interaction::UiLayoutCommand>,
    ) {
        self.front_and_services()
            .0
            .run_ui_layout_commands(prefs, character, world, layout_commands)
    }

    fn split_stack(
        &mut self,
        view: &dereth_client_runtime::hud::HudView<'_>,
        selected: dereth_primitives::ObjectId,
    ) {
        self.front_and_services().0.split_stack(view, selected)
    }

    fn dispatch_input_action(&mut self, action: u32) -> Option<bool> {
        self.front_and_services().0.dispatch_input_action(action)
    }

    fn world_tooltip(&mut self, tooltip: dereth_client_runtime::interaction::WorldTooltip) {
        self.front_and_services().0.world_tooltip(tooltip)
    }

    fn draw_world_target(&mut self, cx: &mut Cx<'_, H>) {
        // The Horizon interface marks the selection itself.
        if self.horizon.active {
            return;
        }
        if self.classic.active {
            let selected = cx.model().selected;
            let projection = selected.and_then(|id| {
                let (present, world) = cx.present_with_world();
                present.target_projection(id, world)
            });
            if let Some(ui) = self.classic.ui.as_mut() {
                ui.draw_world_target(cx, &|id| {
                    if Some(id) == selected {
                        projection
                    } else {
                        None
                    }
                });
            }
            return;
        }
        Ui {
            cx,
            front: &mut self.modern,
            shared: &mut self.shared,
        }
        .draw_world_target();
    }

    fn update_cursor(&mut self, cx: &mut Cx<'_, H>) {
        if let Some(ui) = self.horizon.active() {
            // The game's own cursors, chosen as the modern interface chooses them, over the world
            // only: over the interface the pointer is the plain arrow.
            let over_world = !ui.ui.pointer_over_ui;
            let found = cx.found_object();
            let inputs = crate::cursor::CursorInputs {
                busy: cx.busy_count(),
                target_mode: cx.target_mode().into(),
                combat_mode: cx.model().combat.combat_mode,
                hovering: over_world && found.0 != 0,
                target_compatible: over_world
                    && crate::cursor::is_target_compatible_with_targeting_object(
                        cx.model(),
                        cx.model().targeting_object,
                        found,
                    ),
            };
            let store = std::sync::Arc::clone(cx.store());
            self.shared.cursor.update_cursor_without_ui(&store, inputs);
            return;
        }
        if let Some(ui) = self.classic.active_mut() {
            // The classic interface chooses its pointer and the window system shows it, as it
            // shows this interface's; with none chosen the system's pointer is hidden.
            ui.update_cursor(cx);
            match ui.system_pointer() {
                Some(pointer) => self.shared.cursor.show_picture(
                    pointer.did,
                    (pointer.hot_x, pointer.hot_y),
                    || ui.system_pointer_pixels(pointer),
                ),
                None => self.shared.cursor.hide(),
            }
            return;
        }
        Ui {
            cx,
            front: &mut self.modern,
            shared: &mut self.shared,
        }
        .update_cursor_state();
    }

    /// Copy the completed frame into the UI image, and the mirror `Paste` reads
    /// back: `dereth-ui` records what only a host with a window can perform. Both directions are
    /// cheap -- the send happens only on a Copy, the read only when the clipboard's sequence number
    /// moved.
    fn sync_clipboard(&mut self) {
        if let Some(text) = self
            .horizon
            .active_mut()
            .and_then(dereth_horizon::runtime::HorizonFrontEnd::take_copied)
        {
            use crate::clipboard::HostClipboard;
            if let Err(e) = self.shared.host_clipboard.set_text(&text) {
                tracing::warn!("clipboard write failed: {e:?}");
            }
        }
        if self.classic.active || self.horizon.active {
            return;
        }
        if let Some(s) = self.modern.ui.as_mut() {
            self.shared
                .clipboard
                .sync(&mut s.ui, &mut self.shared.host_clipboard);
        }
    }

    fn compose_ui(&mut self, cx: &mut Cx<'_, H>) {
        let (front, services) = self.front_and_services();
        front.compose(cx, services);
    }

    /// Ending the frame with `true` is three things in one call: **the 2D UI overlay**,
    /// `EndScene` and `Present`. The overlay is composited over the finished 3D frame with the
    /// depth test off, which is why it is inside this step and not a step of its own.
    fn draw_ui(
        &mut self,
        present: &mut Self::Present,
    ) -> Result<(), dereth_client_runtime::present::PresentError> {
        let (front, services) = self.front_and_services();
        front.draw(present, services)
    }

    /// Broadcast the global refresh message — `UiSystem::refresh_event`, which re-lays the root
    /// out at the new extent and pushes `UIGlobalMessage 0x0E` at every registered listener.
    fn set_display(&mut self, display: (i32, i32)) {
        <ModernFrontEnd as FrontEnd<H>>::resize(&mut self.modern, display);
        if let Some(ui) = self.classic.ui.as_mut() {
            <dereth_classic_ui::runtime::ClassicUi as FrontEnd<H>>::resize(ui, display);
        }
    }

    /// UI cleanup: the flow and every root element it holds, then the element manager, with the
    /// texture slots they held handed back first.
    fn cleanup_ui(&mut self, cx: &mut Cx<'_, H>) {
        // A pointer held for a camera drag is shown again before the window goes.
        self.shared.pointer.let_go();
        self.classic.ui = None;
        self.classic.active = false;
        self.horizon.ui = None;
        self.horizon.active = false;
        let ui = self.modern.ui.take();
        if ui.is_some() {
            let r = cx.present_mut().release_ui_textures();
            self.shared.ui_release.freed += r.freed;
            self.shared.ui_release.still_linked += r.still_linked;
            self.shared.ui_release.unknown += r.unknown;
        }
        drop(ui);
    }
}

/// The smart-box object-found notice's tooltip half.
///
/// ```text
///   set wrapper tooltip text to the supplied string
///   enable wrapper tooltip (flag 0x20)
///   if a drag proxy exists:
///       reset the tooltip delay
///       start the wrapper tooltip at the mouse with delay 0.0
///   on the clear arm:
///       disable wrapper tooltip (flags &= 0xFFFFFFDF)
///       clear wrapper tooltip text
/// ```
///
/// **The drag-proxy test is not a suppression — it is the whole reason the direct start
/// exists.** While a drag proxy is up the hover machinery is not running, so nothing would ever
/// start the tooltip for the drop target under the cursor. With no drag, enabling the wrapper
/// tooltip is enough: the hover handler starts it after the ordinary tooltip delay.
fn apply_world_tooltip(
    shell: &mut crate::ui::UiShell,
    call: dereth_client_runtime::interaction::WorldTooltip,
) {
    use dereth_client_runtime::interaction::WorldTooltip;
    let Some(root) = shell
        .flow
        .current()
        .and_then(|s| s.roots().first().copied())
    else {
        return;
    };
    let Some(h) = shell
        .ui
        .get_child_recursive(root, dereth_ui_screens::hud::world_view::SMART_BOX)
    else {
        return;
    };
    match call {
        WorldTooltip::Set {
            name,
            pointer_in_viewport,
        } => {
            shell.ui.set_tooltip(h, Some(name));
            shell.ui.set_tooltip_on(h, true);
            if pointer_in_viewport && shell.ui.drag_state().element.is_some() {
                shell.ui.reset_tooltip();
                shell.ui.start_tooltip_at_mouse(h, 0.0);
            }
        }
        WorldTooltip::Clear => {
            shell.ui.set_tooltip_on(h, false);
            shell.ui.clear_tooltip(h);
        }
    }
}

/// The region-draw visibility test, walked to the root: an element draws only when it and
/// every ancestor is visible.
///
/// `UiSystem::screen_clip_box` intersects the boxes and says nothing about visibility, and a
/// hidden panel keeps its box — so a queue gated on the box alone would draw the doll over the
/// world whenever the backpack was closed.
fn element_is_drawn(ui: &dereth_ui::UiSystem, h: dereth_ui::ElemHandle) -> bool {
    let mut cur = Some(h);
    while let Some(c) = cur {
        match ui.node(c) {
            Some(n) if n.region.flags.visible => cur = ui.parent(c),
            _ => return false,
        }
    }
    true
}

/// Is `hit` — the element the pointer is over — `<SBOX>` or one of its descendants?
///
/// The body of `App::pointer_over_game_view`, taken apart from the point it reads so that the
/// **hover** can ask the same question about its own position. Object picking
/// measures the point it was handed against `render_device`'s viewport and keeps no latch, and
/// the global loop reads the input manager's mouse X/Y live, so the answer belongs to
/// the gesture and not to the frame. Computing it once per frame in `note_game_viewport`, from
/// the *previous* pointer position, would refuse the first hover after the pointer entered the
/// 3-D view as "under the HUD" and arm no pick at all — one wasted frame on every entry, and a
/// whole gesture for a caller that moves and reads in the same frame.
fn pointer_over_game_view_at(
    shell: &crate::ui::UiShell,
    hit: Option<dereth_ui::ElemHandle>,
) -> bool {
    let Some(root) = shell
        .flow
        .current()
        .and_then(|s| s.roots().first().copied())
    else {
        return true;
    };
    let Some(sbox) = shell
        .ui
        .get_child_recursive(root, dereth_ui_screens::hud::world_view::SMART_BOX)
    else {
        return true;
    };
    let ui = &shell.ui;
    let Some(mut h) = hit else { return true };
    loop {
        if h == sbox {
            return true;
        }
        match ui.parent(h) {
            Some(p) => h = p,
            None => return false,
        }
    }
}

/// An object notice offered to the panels that watch objects, and the requests they raised on
/// the way, in order.
fn dispatch_object_panel_notice(
    shell: Option<&mut crate::ui::UiShell>,
    hud: &mut crate::hud::Hud,
    world: &dereth_client_model::World,
    notice: &dereth_client_model::Notice,
) -> Vec<dereth_client_contract::UiRequest> {
    let Some(shell) = shell else {
        return Vec::new();
    };
    let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) else {
        return Vec::new();
    };
    let boundary = shell.ui.requests.len();
    hud.object_notice(&mut shell.ui, screen, world, notice);
    shell.ui.requests.take_since(boundary)
}

/// One host event, routed: a lifecycle event to the runtime's window procedure (and its
/// messages on to the input manager, which releases every held control on a focus loss), a
/// device event through the residual flycam and mouse-look latches, the window procedure's
/// device arms (the Alt+Enter toggle among them) and the input manager.
///
/// A close request is always honoured, Alt+F4's included: the window procedure lets Alt+F4 through
/// to the default procedure, whose answer is the close.
pub(crate) fn route_host_event<H: Host>(
    cx: &mut Cx<'_, H>,
    shell: &mut ClientShell<H>,
    event: &dereth_input::host::HostEvent,
    time_ms: u32,
) {
    use dereth_input::host::HostEvent;

    if let Some(lifecycle) = crate::platform::window::lifecycle(event) {
        cx.window_event(shell, &lifecycle, time_ms);
        if let Some(input) = shell.shared.input.as_mut() {
            // `wnd_proc_disposition` inside the input manager applies the window procedure's
            // table again and drops everything it does not forward.
            for m in dereth_client_runtime::pump::Pump::map_window_event(&lifecycle) {
                input.on_message(crate::pump::from_window(m, time_ms));
            }
        }
        return;
    }
    match event {
        HostEvent::KeyboardInput { key, pressed, .. } => flycam_key(cx, shell, *key, *pressed),
        HostEvent::MouseInput {
            button: dereth_input::keys::MouseButton::Right,
            pressed,
        } => cx.mouse_look_button(*pressed),
        HostEvent::CursorMoved { x, y } => cx.cursor_moved(*x, *y),
        _ => {}
    }
    // The window procedure runs its message table, then packages
    // `hwnd/message/wParam/lParam/GetMessageTime()` into a `MSG` for the input manager's
    // message handler, whose tap and double-click thresholds are driven by that time.
    for m in shell.shared.devices.map_device_event(event, time_ms) {
        cx.window_message(crate::pump::window_message(m), m.time_ms);
        if let Some(input) = shell.shared.input.as_mut() {
            input.on_message(m);
        }
        Ui {
            cx,
            front: &mut shell.modern,
            shared: &mut shell.shared,
        }
        .input_message();
    }
}

/// One host event while the Horizon interface is shown: a lifecycle event as any interface routes it;
/// a device event to the interface first, and on to the latches, the window procedure and the
/// input manager only when the interface does not keep it.
fn route_horizon_event<H: Host>(
    cx: &mut Cx<'_, H>,
    shell: &mut ClientShell<H>,
    event: &dereth_input::host::HostEvent,
    time_ms: u32,
) {
    use dereth_input::host::HostEvent;
    if let Some(lifecycle) = crate::platform::window::lifecycle(event) {
        cx.window_event(shell, &lifecycle, time_ms);
        if let Some(input) = shell.shared.input.as_mut() {
            for m in dereth_client_runtime::pump::Pump::map_window_event(&lifecycle) {
                input.on_message(crate::pump::from_window(m, time_ms));
            }
        }
        return;
    }
    // A paste key in a text box reads the clipboard on the keystroke.
    if let HostEvent::KeyboardInput {
        key, pressed: true, ..
    } = event
    {
        if let Some(ui) = shell.horizon.active_mut() {
            if ui.wants_paste(key.virtual_key) {
                use crate::clipboard::HostClipboard;
                let text = shell.shared.host_clipboard.get_text().ok().flatten();
                ui.offer_paste(text);
            }
        }
    }
    let forward = shell
        .horizon
        .active_mut()
        .is_none_or(|ui| ui.host_event(event, time_ms).forward);
    match event {
        HostEvent::CursorMoved { x, y } => cx.cursor_moved(*x, *y),
        // Either button held over the world turns the camera with the pointer, and both together
        // run forward; letting go ends it wherever the pointer is.
        HostEvent::MouseInput {
            button:
                button
                @ (dereth_input::keys::MouseButton::Left | dereth_input::keys::MouseButton::Right),
            pressed,
        } if forward || !*pressed => {
            cx.orbit_button(*button == dereth_input::keys::MouseButton::Right, *pressed);
        }
        // The middle button over the world locks or frees the run.
        HostEvent::MouseInput {
            button: dereth_input::keys::MouseButton::Middle,
            pressed: true,
        } if forward => {
            use dereth_client_runtime::actions::{Action, ActionId};
            const AUTORUN: ActionId = dereth_client_contract::actions::movement::AUTORUN;
            cx.inject_action(Action::begin(AUTORUN));
            cx.inject_action(Action::end(AUTORUN));
        }
        HostEvent::KeyboardInput { key, pressed, .. } if forward => {
            flycam_key(cx, shell, *key, *pressed);
        }
        _ => {}
    }
    if !forward {
        return;
    }
    for m in shell.shared.devices.map_device_event(event, time_ms) {
        cx.window_message(crate::pump::window_message(m), m.time_ms);
        if let Some(input) = shell.shared.input.as_mut() {
            input.on_message(m);
        }
    }
}

/// The residual flycam's two keys. See [`App::flycam_key`](crate::app::App::flycam_key).
pub(crate) fn flycam_key<H: Host>(
    cx: &mut Cx<'_, H>,
    shell: &ClientShell<H>,
    key: dereth_input::keys::Key,
    down: bool,
) {
    use dereth_input::keys::Key;
    if shell
        .shared
        .input
        .as_ref()
        .is_some_and(crate::input::InputShell::keyboard_blocked)
    {
        return;
    }
    if key == Key::SPACE {
        cx.flycam_rise(down);
    } else if key == Key::KEY_C {
        cx.flycam_sink(down);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The key the char-gen turntable is gated on must carry the two `UIASSET` enums the gated
    /// block reads, and carrying them must **not** make an animation change rebuild the space.
    ///
    /// Oracle: the preview update tail — the setup-changed and
    /// background-setup-changed tests decide the rebuild; the animation start and
    /// stop are separate calls that only re-issue the sequence.
    ///
    /// Falsified by: dropping either enum from [`ChargenPreviewKey`] (the first two arms fail), or
    /// by putting either of them into [`ChargenPreviewKey::same_space`] (the third fails, and every
    /// animation change would tear the model down and reload it).
    #[cfg(gpu)]
    #[test]
    fn the_chargen_preview_key_carries_both_animation_enums_without_forcing_a_rebuild() {
        use dereth_ui_screens::screens::chargen::Cg3dView;

        // The key is built from the view the gated block reads, through the same constructor the
        // frame uses -- so this asserts the *correspondence* and not merely that a struct with six
        // fields compares on six fields.
        let v = Cg3dView {
            setup: DataId(0x0200_0001),
            bg_setup: DataId(0x0200_05A4),
            animating: false,
            animation_enum: 0x1000_0011,
            rest_animation_enum: 0x1000_0012,
            ..Cg3dView::default()
        };
        let od = dereth_animation::parts::ObjDesc::default;
        let base = ChargenPreviewKey::from_view(&v, od());

        // The wizard moves to a heritage whose animation enums differ. Nothing else about the
        // view changes, and the key must follow -- this is the arm that was missing.
        let moved_view = Cg3dView {
            animation_enum: 0x1000_0021,
            ..v.clone()
        };
        assert_ne!(
            base,
            ChargenPreviewKey::from_view(&moved_view, od()),
            "the view's animation enum moved and the key did not take it"
        );
        let rested_view = Cg3dView {
            rest_animation_enum: 0x1000_0022,
            ..v.clone()
        };
        assert_ne!(
            base,
            ChargenPreviewKey::from_view(&rested_view, od()),
            "the view's rest animation enum moved and the key did not take it"
        );
        // A heritage whose animation enums differ from the previous one's. On the shipped table
        // that is Olthoi / OlthoiAcid, whose setups also differ -- but their `environment_setup` is
        // the same `0x020005A4` and both sexes inside each share one setup, so this table
        // demonstrably reuses ids and the containment is data rather than construction.
        let moved_anim = ChargenPreviewKey {
            animation_enum: 0x1000_0021,
            ..base.clone()
        };
        let moved_rest = ChargenPreviewKey {
            rest_animation_enum: 0x1000_0022,
            ..base.clone()
        };
        assert_ne!(
            base, moved_anim,
            "the animation enum moved and the sequence was not re-issued"
        );
        assert_ne!(
            base, moved_rest,
            "the rest animation enum moved and nothing re-issued"
        );

        // …and neither of them is a rebuild: the model stays, the sequence changes.
        assert!(
            base.same_space(&moved_anim),
            "an animation change tore down the preview space"
        );
        assert!(
            base.same_space(&moved_rest),
            "a rest-animation change tore down the space"
        );

        // The three that *are* a rebuild, so `same_space` is not vacuously true.
        assert!(!base.same_space(&ChargenPreviewKey {
            setup: DataId(0x0200_1A21),
            ..base.clone()
        }));
        assert!(!base.same_space(&ChargenPreviewKey {
            bg_setup: DataId(0x0200_1A20),
            ..base.clone()
        }));
        let dressed = dereth_animation::parts::ObjDesc {
            palette_id: DataId(0x0400_0001),
            ..dereth_animation::parts::ObjDesc::default()
        };
        assert!(!base.same_space(&ChargenPreviewKey {
            objdesc: dressed,
            ..base.clone()
        }));

        // And the animating flag itself, the term the key has always carried.
        assert_ne!(
            base,
            ChargenPreviewKey {
                animating: true,
                ..base.clone()
            }
        );
    }
}

#[cfg(test)]
#[path = "../tests/message_tests.rs"]
mod message_tests;

#[cfg(test)]
#[path = "../tests/interface_switch_tests.rs"]
mod interface_switch_tests;

#[cfg(test)]
#[path = "../tests/patch_reread_tests.rs"]
mod patch_reread_tests;

#[cfg(test)]
#[path = "../tests/classic_text_tests.rs"]
mod classic_text_tests;

#[cfg(test)]
#[path = "../tests/horizon_switch_tests.rs"]
mod horizon_switch_tests;

#[cfg(test)]
#[path = "../tests/pointer_hold_tests.rs"]
mod pointer_hold_tests;
