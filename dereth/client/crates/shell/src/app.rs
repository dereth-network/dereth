//! The executable's front end: the retail UI, the cursor, the clipboard, the preview spaces and
//! the overlay, plugged into [`dereth_client_runtime::app::App`]'s frame.
//!
//! The frame loop is the runtime's. What is here is [`ClientShell`], the [`Shell`] the executable
//! supplies, and [`App`], the application with that front end in it: the runtime's application
//! beside its shell, so each frame hands the one to the other. `App` dereferences to the runtime's
//! application, so every read and every step that needs no UI is the runtime's own method.
//!
//! The main loop is tiny: connect once, then run one frame at a time until a frame asks to stop.
//! [`App::run`] is that loop and [`App::frame`] is one frame of it.

pub use dereth_client_runtime::app::*;

use dereth_primitives::{AssetSource, DataId};

use crate::platform::host::Host;
use crate::{config::Config, present::ClientPresentation};

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
    /// **The denominator**: bindable `(input map, action)` pairs in the merged action map.
    /// `rows_built` must equal it.
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

/// What the 3D character preview space was last built and animated for.
///
/// # What this key is a superset OF
///
/// The gated block does two things and this key holds the inputs of both. It **rebuilds** the space
/// when the model setup changes, when the background setup changes,
/// or when the dressed `ObjDesc` changes — [`Self::same_space`], the
/// three fields that decide a rebuild. It then **re-issues the sequence** from the animating state
/// (starting animation or parking it) and from whichever of
/// `CharGenScreen::view3d`'s two `UIASSET` enum values that arm selects.
///
/// # Why the two animation enums are in the key
///
/// [`Self::animation_enum`] and [`Self::rest_animation_enum`] are read by the body this key
/// gates. Without them, a heritage change that moved the animation enums without moving the
/// setup would leave the turntable playing the previous heritage's animation.
///
/// It cannot happen on the shipped table (`CharGen_CharacterData 0x0E000002`, checked by a test).
/// Character-generation preview initialization selects between **three** enum groups (default,
/// Olthoi `0x0C`, OlthoiAcid `0x0D`), all three distinct, and the two Olthoi heritages' setups
/// are `0x02001A21` and `0x02001A20` — different, so a key without the enums would still fire
/// on every reachable transition.
///
/// **The near miss is why the enums are in the key anyway:** those two heritages
/// share one `environment_setup` (`0x020005A4`), and both sexes inside each Olthoi heritage share
/// one setup — so the heritage table demonstrably *does* reuse ids across rows, and containment by
/// "the shipped data happens not to collide on this column" is a property of the dats, not of this
/// code. A DDD patch, or a heritage row nobody has looked at, ends it silently.
///
/// This is deliberately **not** the audio guard's case: retail's update re-reads the enums from the same
/// heritage-table row every call, and this rebuild assumes nothing more than the client does. The
/// key is simply a projection of its own body.
#[derive(Debug, Clone, PartialEq)]
struct ChargenPreviewKey {
    /// The model setup id.
    setup: DataId,
    /// Whether the preview is animating.
    animating: bool,
    /// The background setup id from the heritage row's environment setup.
    bg_setup: DataId,
    /// The **dressed** part array, because the preview's meshes are baked from it.
    objdesc: dereth_animation::parts::ObjDesc,
    /// The `UIASSET` enum played while `animating`.
    animation_enum: u32,
    /// …and the one used for the parked pose when it is not.
    rest_animation_enum: u32,
}

impl ChargenPreviewKey {
    /// Every field, taken off the view the gated block itself reads.
    ///
    /// A constructor rather than a struct literal at the call site so that the *correspondence*
    /// between the key's fields and `Cg3dView`'s has a station: a test can build a `Cg3dView`, move
    /// one field, and require the key to follow — which is the half a struct-equality assertion
    /// cannot see, since a derived `PartialEq` compares whatever fields happen to be there.
    fn from_view(
        v: &dereth_ui_screens::screens::chargen::Cg3dView,
        objdesc: dereth_animation::parts::ObjDesc,
    ) -> Self {
        Self {
            setup: v.setup,
            animating: v.animating,
            bg_setup: v.bg_setup,
            objdesc,
            animation_enum: v.animation_enum,
            rest_animation_enum: v.rest_animation_enum,
        }
    }

    /// The three fields that decide a **rebuild** of the space, as opposed to a re-issue of the
    /// sequence. The animation enums are deliberately outside this: changing which animation plays
    /// must not tear down and reload the model, which is exactly what starting and
    /// stopping the animation being separate calls at the end of the preview update means.
    fn same_space(&self, other: &Self) -> bool {
        self.setup == other.setup
            && self.bg_setup == other.bg_setup
            && self.objdesc == other.objdesc
    }
}

/// How far `--enter-world` has driven the character-management and character-generation screens.
///
/// The retail client has no script: the *screen* is what enters the world, and `-u`/`-user` and
/// `-r`/`-create` are what the launcher passes to say which character. `--enter-world` presses
/// the screen's own buttons instead of calling the session directly, so a
/// headless run exercises the path a player does rather than a parallel one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PregameDrive {
    /// Nothing pressed yet.
    #[default]
    Idle,
    /// *Create Character* has been pressed; waiting for the wizard.
    OpeningWizard,
    /// The heritage, town and summary page have been chosen; the name and *Finish* follow on the
    /// next frame, once the summary page is up.
    Naming,
    /// The wizard has been filled in and *Finish* pressed; waiting for the credit warning, which
    /// the wizard's finish step raises and which cannot be seen until the click has been
    /// delivered — the element messages are queued, so the answer is a second frame's work.
    Finishing,
    /// The credit warning has been answered and `0xF656` has gone out.
    Creating,
    /// A row has been double-clicked; `EnterGame` has run.
    Entering,
}

/// The object `--cast` selects before casting `spell_id`: none for a spell cast on the caster or
/// on no one, else the player when the client's target test takes it, else the first worn, then
/// carried, item it takes.
fn scripted_cast_target(
    world: &dereth_client_model::world::World,
    spell_id: u32,
) -> Option<dereth_primitives::ObjectId> {
    let table = world.magic.spell_table.as_ref()?;
    let base = table.spells.get(&spell_id)?;
    let target_type = dereth_client_model::magic::spell_target_type(base);
    if base.bitfield & dereth_client_model::magic::spell_index::SELF_TARGETED != 0
        || target_type == 0
    {
        return None;
    }
    let player = world.player?;
    let mut candidates = vec![player];
    if let Some(inv) = world.inventory(player) {
        candidates.extend(inv.placements.iter().map(|p| p.iid));
        candidates.extend(inv.items.iter().copied());
    }
    candidates.into_iter().find(|&id| {
        world
            .object_compatible_with_spell_target_type(
                &mut dereth_client_model::NullSink,
                Some(id),
                target_type,
                true,
            )
            .is_ok()
    })
}

/// The first caster (wand, staff or orb) the player carries, which `--cast` wields to enter
/// magic mode as a player would.
fn scripted_caster(
    world: &dereth_client_model::world::World,
) -> Option<dereth_primitives::ObjectId> {
    let inv = world.inventory(world.player?)?;
    inv.items.iter().copied().find(|&id| {
        world
            .weenie(id)
            .is_some_and(|w| w.inq_type() & dereth_client_model::weenie::item_type::CASTER != 0)
    })
}

/// How far `--cast` has driven the world screen: the backpack opened, a caster wielded, magic
/// mode entered, the target selected, the spell cast, peace again, then the closest compass item
/// used.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum WorldDrive {
    /// Not in the world yet.
    #[default]
    Idle,
    /// In the world since this time; the inventory and the materialization settle first.
    Arrived(f64),
    /// The backpack button was pressed at this time.
    BackpackOpened(f64),
    /// The carried caster was used (wielded) at this time.
    CasterWielded(f64),
    /// The combat toggle was pressed at this time.
    MagicMode(f64),
    /// The target, if the spell needs one, was selected at this time.
    TargetChosen(f64),
    /// The cast was asked for at this time.
    Cast(f64),
    /// The combat toggle was pressed again (back to peace) at this time.
    Peace(f64),
    /// The closest compass item (the nearest person) was selected at this time.
    Approaching(f64),
    /// Walking up to it; nothing more to do.
    Done,
}

/// Complete every implemented request owner in order, at BOTH input-listener and HUD-update
/// boundaries. The presentation is a live field-disjoint borrow: the scene needs no copy and no
/// extra advancement, and the device-owned preference names land through the same handle.
#[allow(clippy::too_many_arguments)]
fn dispatch_ui_owner_requests(
    shell: &mut crate::ui::UiShell,
    interaction: &mut crate::interaction::Interaction,
    objects: &mut crate::objects::ObjectStream,
    hud: &mut crate::hud::Hud,
    targeted_dialogs: &mut crate::target_confirmation::TargetedDialogs,
    mut audio: Option<&mut crate::audio::Audio>,
    present: &mut dyn crate::present::Presentation,
    world: &mut Option<dereth_client_runtime::world_state::WorldState>,
    serial: u64,
    now: dereth_primitives::LocalTime,
) -> Vec<dereth_ui_screens::view::UiRequest> {
    use crate::interaction::TargetMode;
    use dereth_ui_screens::screens::gameplay_host::GameCall;
    targeted_dialogs.service(Some(shell), interaction, &mut objects.world, now);
    let active = interaction.target_mode() != TargetMode::None;
    shell.set_target_mode_active(active);
    if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
        hud.dispatch_panel_input(&mut shell.ui, screen, serial, objects, active);
    }
    let mut unowned = Vec::new();
    loop {
        let pending = shell.ui.requests.take();
        if pending.is_empty() {
            break;
        }
        let mut pending: std::collections::VecDeque<_> = pending.into();
        while let Some(request) = pending.pop_front() {
            // Shell and placement owners currently emit no direct request descendants. Their
            // common tail is still run: SetLockUi is delegated to Interaction's authoritative
            // option write before App completes its broadcast, and later direct descendants must
            // not silently fall behind it.
            let request = match request {
                dereth_ui_screens::view::UiRequest::BarberLocalEffect(script) => {
                    if let Some(mut scene) = present.scene_mut(world.as_mut()) {
                        scene.replace_player_particle_script(script);
                    }
                    None
                }
                dereth_ui_screens::view::UiRequest::BarberLocalMotionTable(table) => {
                    if let Some(mut scene) = present.scene_mut(world.as_mut()) {
                        scene.world_mut().replace_player_motion_table(table);
                    }
                    None
                }
                request => shell.handle_request(request),
            };
            if let Some(request) = request {
                let mut request = vec![request];
                hud.consume_placement_requests(
                    &mut objects.world,
                    &mut request,
                    dereth_primitives::ServerTime(now.0),
                );
                if !request.is_empty() {
                    // The player's position, for `@loc`: the
                    // one chat command that reads the body. Stamped per dispatch, the way
                    // `dispatch_ui_selection_notices` reads the same origin, because the handler
                    // below runs against `dereth_client_model::World` and the body lives on the scene.
                    interaction.player_position = world
                        .as_ref()
                        .and_then(|w| w.character.as_ref())
                        .map(crate::character::Character::position);
                    interaction.queue(Vec::new(), request);
                    let remaining = interaction.run_ui_requests_with_chat_focus(
                        &mut objects.world,
                        hud.player_desc_received,
                        dereth_primitives::ServerTime(now.0),
                        &mut |chat| {
                            let notices = chat.take_talk_focus_notices();
                            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                                crate::hud::deliver_chat_focus_notices(
                                    &mut shell.ui,
                                    screen,
                                    chat,
                                    notices,
                                );
                            }
                        },
                    );
                    let remaining =
                        crate::audio::apply_preference_requests(audio.as_deref_mut(), remaining);
                    unowned.extend(present.apply_device_preference_requests(remaining));
                }
            }
            interaction.dispatch_ui_selection_notices(
                world.as_ref().and_then(|w| w.character.as_ref()),
                objects,
                now,
            );
            targeted_dialogs.service(Some(shell), interaction, &mut objects.world, now);
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    screen,
                    GameCall::InteractionContext {
                        target_mode: interaction.target_mode() != TargetMode::None,
                        selected: objects.world.selected,
                    },
                );
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
                target_mode: interaction.target_mode() != TargetMode::None,
                selected: objects.world.selected,
            },
        );
        hud.refresh_item_input_views(&mut shell.ui, screen, objects);
    }
    shell.set_target_mode_active(interaction.target_mode() != TargetMode::None);
    unowned
}

/// The executable's front end: everything the frame hands the UI, and the state the UI keeps
/// across frames.
pub struct ClientShell<H: Host> {
    /// How far `--enter-world` has driven the pre-game screens. See `App::drive_pregame_screens`.
    pregame: PregameDrive,
    /// How far `--cast` has driven the world screen. See `App::drive_world_script`.
    world_drive: WorldDrive,
    /// When `--say` last sent a line or `--use` last acted (or may first), and whether the
    /// closest item has been selected and waits to be used. See `App::drive_say`.
    say_drive: Option<f64>,
    say_selected: bool,
    /// The client UI cursor half: the current cursor id, the built
    /// `HCURSOR`s, and the window they are installed on. See [`crate::cursor`].
    cursor: crate::cursor::CursorSystem,
    /// The clipboard bridge: the Win32 hop `dereth-ui` cannot make, and the
    /// sequence number that keeps the per-frame refresh from taking the desktop clipboard lock.
    clipboard: crate::clipboard::ClipboardBridge,
    /// The host's clipboard, which the bridge mirrors.
    host_clipboard: H::Clipboard,

    /// the current cursor id plus the cursor image. `None` without `--ui`.
    ui: Option<crate::ui::UiShell>,
    /// The device input: the input manager and the registrations the client's systems make.
    /// Present whenever the input tables loaded, with or without `--ui`.
    input: Option<crate::input::InputShell>,
    /// The device half of the message mapping: the Alt state and the pointer position the button
    /// messages carry.
    devices: crate::pump::DeviceMessages,
    /// The window's queue of events, routed at the event-loop step.
    window_events: crate::platform::window::WindowEvents,
    /// What every `release_ui_textures` has done so far. `unknown` is a double release and is
    /// asserted zero.
    ui_release: crate::gpu::UiReleaseReport,
    /// This frame's 2D blit list, built at step 7 and drawn inside `PresentFrame`.
    ui_draw_list: Vec<dereth_ui::UiDrawCmd>,
    targeted_dialogs: crate::target_confirmation::TargetedDialogs,
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
    key_binding_stats: KeyBindingStats,
    /// Time at the previous character-generation preview tick. Preview animations advance from
    /// **elapsed seconds** and accumulate nothing per frame; see
    /// [`crate::preview::PreviewSpace::use_time`].
    preview_last_time: f64,
    /// Time at the previous portal-preview tick, which advances independently.
    portal_last_time: f64,
    /// Time at the previous paper-doll preview tick.
    paper_doll_last_time: f64,
    /// Time at the previous identify-window portrait tick.
    examine_3d_last_time: f64,
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
    chargen_pal_sets: crate::preview::PaletteSetCache,
    /// What the last `chargen_objdesc` reached, so a test can assert the block ran
    /// rather than inferring it from an empty descriptor. See
    /// [`crate::preview::ChargenDressStats`].
    chargen_dress: crate::preview::ChargenDressStats,
    /// What the paper doll was last built for: the player's
    /// setup record and the `ObjDesc` used to redress the creature.
    ///
    /// The client keeps the clone for ever and reapplies only descriptor changes when the
    /// appearance changes; here the descriptor is part of the key because the part meshes are
    /// baked from the dressed array (see
    /// [`crate::preview::PreviewSpace::add_object_dressed`]), so a redress is a rebuild.
    paper_doll_built: Option<(DataId, dereth_animation::parts::ObjDesc)>,
    /// The paper-doll panel's flip count, next-flip time, and selection mask —
    /// the doll's selection blink. See [`crate::preview::PaperDollSelectionLighting`].
    paper_doll_lighting: crate::preview::PaperDollSelectionLighting,
    /// The selection last observed by the paper doll. `dereth_client_model` records selection broadcasts
    /// instead of raising them directly, so this edge is delivered with the same one-frame delay
    /// as the other subscriber. Re-selecting the unchanged id broadcasts without an edge and does
    /// not restart the blink.
    paper_doll_selection_seen: Option<dereth_primitives::ObjectId>,
}

impl<H: Host> std::fmt::Debug for ClientShell<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientShell")
            .field("ui", &self.ui.is_some())
            .field("gameplay_serial", &self.gameplay_serial)
            .finish_non_exhaustive()
    }
}

impl<H: Host> ClientShell<H> {
    /// A front end whose cursor is installed on `hwnd` by host `H`, with no UI up yet.
    #[must_use]
    pub fn new(hwnd: Option<isize>) -> Self {
        Self::with_window_events(hwnd, crate::platform::window::WindowEvents::default())
    }

    /// A front end that routes the events the window queues on `window_events`.
    #[must_use]
    pub fn with_window_events(
        hwnd: Option<isize>,
        window_events: crate::platform::window::WindowEvents,
    ) -> Self {
        Self {
            input: None,
            devices: crate::pump::DeviceMessages::default(),
            window_events,
            pregame: PregameDrive::default(),
            world_drive: WorldDrive::default(),
            say_drive: None,
            say_selected: false,
            cursor: crate::cursor::CursorSystem::with_images(H::cursor_images(hwnd)),
            clipboard: crate::clipboard::ClipboardBridge::default(),
            host_clipboard: H::clipboard(),
            ui: None,
            ui_release: crate::gpu::UiReleaseReport::default(),
            ui_draw_list: Vec::new(),
            targeted_dialogs: crate::target_confirmation::TargetedDialogs::default(),
            gameplay_serial: 0,
            key_bindings_built: None,
            key_binding_stats: KeyBindingStats::default(),
            preview_last_time: 0.0,
            portal_last_time: 0.0,
            paper_doll_last_time: 0.0,
            examine_3d_last_time: 0.0,
            examine_3d_built: None,
            preview_chargen: None,
            chargen_pal_sets: crate::preview::PaletteSetCache::default(),
            chargen_dress: crate::preview::ChargenDressStats::default(),
            paper_doll_built: None,
            paper_doll_lighting: crate::preview::PaperDollSelectionLighting::default(),
            paper_doll_selection_seen: None,
        }
    }
}

/// The runtime's application with this executable's front end in it.
pub type CoreApp<H> = dereth_client_runtime::app::App<ClientShell<H>>;

/// The application: the runtime's, beside the front end its frames hand the UI steps to, on
/// host `H`.
pub struct App<H: Host> {
    core: CoreApp<H>,
    shell: ClientShell<H>,
}

impl<H: Host> std::ops::Deref for App<H> {
    type Target = CoreApp<H>;
    fn deref(&self) -> &CoreApp<H> {
        &self.core
    }
}

impl<H: Host> std::ops::DerefMut for App<H> {
    fn deref_mut(&mut self) -> &mut CoreApp<H> {
        &mut self.core
    }
}

impl<H: Host> std::fmt::Debug for App<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.core, f)
    }
}

impl<H: Host> App<H> {
    /// Run startup steps 8 through 12 in order, with absent subsystems named, on the device
    /// presentation.
    ///
    /// Preferences (step 8) were loaded by [`Config::from_args_and_prefs_at`]; database
    /// initialization (step 10) and UI initialization (step 12) run in the documented order,
    /// because the database must precede the UI (the UI layouts are dat objects).
    ///
    /// # Errors
    /// [`StartupError`] for any of the failures the client treats as fatal.
    #[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
    pub fn new(cfg: Config) -> Result<Self, StartupError> {
        Self::bring_up(cfg, None, None)
    }

    /// The same bring-up with the presentation supplied.
    ///
    /// This is the ungated constructor: it is what a headless `App` is built with
    /// ([`crate::present::NullPresentation`]), and it is what a second backend is handed to.
    /// [`App::new`] is this function with the device presentation built for it.
    ///
    /// # Errors
    /// [`StartupError`] for any of the failures the client treats as fatal.
    pub fn with_presentation(
        cfg: Config,
        present: Box<dyn ClientPresentation>,
    ) -> Result<Self, StartupError> {
        Self::bring_up(cfg, Some(present), None)
    }

    /// The headless bring-up in full: a presentation, a window and a clock, none of which needs a
    /// device or a window system.
    ///
    /// `NullPresentation` + `NullWindow` + `FixedStepClock` is *the* headless configuration, and
    /// this is the one constructor that says so in one place.
    ///
    /// # Errors
    /// [`StartupError`] for any of the failures the client treats as fatal.
    pub fn with_platform(
        cfg: Config,
        present: Box<dyn ClientPresentation>,
        platform: Platform,
    ) -> Result<Self, StartupError> {
        Self::bring_up(cfg, Some(present), Some(platform))
    }

    /// The runtime's twelve startup steps, with this executable's platform and presentation
    /// supplied at the steps that build them, and the front end beside the result.
    fn bring_up(
        cfg: Config,
        present: Option<Box<dyn ClientPresentation>>,
        platform: Option<Platform>,
    ) -> Result<Self, StartupError> {
        // The host's answers the runtime cannot compute itself: the local zone, the URL launch,
        // the caret blink and the default audio endpoint.
        crate::hud::install_platform::<H>();
        H::install_default_output();
        // The window's events, shared by the window that queues them and the front end that
        // routes them. A platform supplied from outside queues none.
        let window_events = crate::platform::window::WindowEvents::default();
        let opened_events = std::rc::Rc::clone(&window_events);
        Self::bring_up_with_store(
            cfg,
            None,
            window_events,
            |cfg| match platform {
                Some(p) => Ok(p),
                None if cfg.headless => Ok(Platform::headless(cfg.width, cfg.height)),
                None => H::open_platform(cfg, opened_events),
            },
            |window, client_w, client_h, cfg| match present {
                Some(p) => Ok(p),
                #[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
                None => Self::device_presentation(window, client_w, client_h, cfg),
                #[cfg(not(any(
                    feature = "vulkan",
                    feature = "wgpu",
                    all(windows, feature = "d3d12")
                )))]
                None => {
                    let _ = (window, cfg);
                    Ok(Box::new(crate::present::NullPresentation::new(
                        client_w, client_h,
                    )))
                }
            },
        )
    }

    /// Bring up this front end over a host-opened data store and host-provided factories.
    /// The host installs its clock-zone, URL, caret and audio services before this call.
    ///
    /// # Errors
    /// A startup failure from the runtime, platform or presentation factory.
    pub fn bring_up_with_store(
        cfg: Config,
        store: Option<std::sync::Arc<dereth_dat::RetailDatStore>>,
        window_events: crate::platform::window::WindowEvents,
        platform: impl FnOnce(&Config) -> Result<Platform, StartupError>,
        present: impl FnOnce(
            &dyn crate::platform::window::WindowHost,
            u32,
            u32,
            &Config,
        ) -> Result<Box<dyn ClientPresentation>, StartupError>,
    ) -> Result<Self, StartupError> {
        let mut core = CoreApp::<H>::bring_up_with_store(cfg, store, platform, present)?;
        core.interaction.client_build_id = H::BUILD_ID;
        let shell = ClientShell::with_window_events(core.window.raw_handle(), window_events);
        Ok(Self { core, shell })
    }

    /// Run the main loop: frame after frame until one asks to stop. Returns the number of frames drawn.
    pub fn run(&mut self) -> u64 {
        self.core.run(&mut self.shell)
    }

    /// One iteration of the frame, in the documented order. Returns false when the loop should
    /// end.
    pub fn frame(&mut self) -> bool {
        self.core.frame(&mut self.shell)
    }

    /// Start input, sound, and the UI shell in the client's order.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the UI's own two dat objects are missing.
    pub fn start_shell(&mut self) -> Result<(), StartupError> {
        self.core.start_shell(&mut self.shell)
    }

    /// Run the normal cleanup path, in [`crate::shutdown::Step::ORDER`].
    pub fn shutdown(self) -> crate::shutdown::CleanupLog {
        let Self { core, mut shell } = self;
        core.shutdown(&mut shell)
    }

    /// The HUD's event application against this application's own object tables, as the frame
    /// makes it.
    pub fn apply_hud_events(
        &mut self,
        events: &[dereth_client_net::client_session::SessionEvent],
    ) -> Vec<dereth_client_contract::chat::interface::ChatMessage> {
        self.core.apply_hud_events(&mut self.shell, events)
    }

    /// Act on what the session decoded, as the frame does.
    pub fn process_logon_event_queue(
        &mut self,
        events: Vec<dereth_client_net::client_session::SessionEvent>,
    ) {
        self.core.process_logon_event_queue(&mut self.shell, events);
    }

    /// The interaction boundary as the frame runs it, with the panels callback.
    pub fn apply_interaction_events_to_panels(
        &mut self,
        events: &[dereth_client_net::client_session::SessionEvent],
    ) {
        self.core
            .apply_interaction_events_to_panels(&mut self.shell, events);
    }

    /// Force display resolution with `(force, width, height)`.
    pub fn force_display_resolution(&mut self, force: bool, width: u32, height: u32) {
        self.core
            .force_display_resolution(&mut self.shell, force, width, height);
    }

    /// Load the static scene: one landblock's terrain and scenery, plus the LOD window around it.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the region, the landblock or a device resource is unavailable.
    pub fn load_static_scene(
        &mut self,
        cfg: crate::world::SceneConfig,
    ) -> Result<(), StartupError> {
        self.core.load_static_scene(cfg)
    }

    /// One host event, as the event loop consumes it: a lifecycle event reaches the runtime's
    /// window procedure, a device event this client's input.
    pub fn handle_window_event(
        &mut self,
        event: &crate::platform::window::HostEvent,
        time_ms: u32,
    ) {
        route_host_event(&mut self.core, &mut self.shell, event, time_ms);
    }

    /// The device input, for the tests and the console's state line.
    pub fn input_manager_mut(&mut self) -> Option<&mut crate::input::InputShell> {
        self.shell.input.as_mut()
    }

    /// The device input, read-only.
    #[must_use]
    pub fn input_manager(&self) -> Option<&crate::input::InputShell> {
        self.shell.input.as_ref()
    }

    /// One key transition of the residual flycam: `Space` raises it and `C` lowers it, unless a
    /// keyboard barrier stands (a focused text box stops these two keys for the same reason it
    /// stops every other one). Every other key is not the flycam's.
    pub fn flycam_key(&mut self, key: crate::platform::keys::Key, down: bool) {
        flycam_key(&mut self.core, &self.shell, key, down);
    }

    /// The runtime's own focus-loss latches, for a lifecycle event; a device event has none.
    pub fn note_flycam_input(&mut self, event: &crate::platform::window::HostEvent) {
        if let Some(lifecycle) = crate::platform::window::lifecycle(event) {
            self.core.note_flycam_input(&lifecycle);
        }
    }

    /// Step 11 for the gated [`App::new`] path: construct the graphics engine and apply the three
    /// presentation preferences it needs before anything draws.
    #[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
    fn device_presentation(
        window: &dyn crate::platform::window::WindowHost,
        client_w: u32,
        client_h: u32,
        cfg: &Config,
    ) -> Result<Box<dyn ClientPresentation>, StartupError> {
        let handles = window.render_handles();
        let mut renderer = Self::device_on_selected_backend(handles, client_w, client_h, cfg)?;
        // Startup registers the render variable over preloaded preference shadows. Apply
        // the configured profile before any world/UI draw; no owner settings are written.
        renderer.set_texture_filtering(crate::render_prefs::texture_filtering_from_file(
            &cfg.preferences_file,
        ));
        // `Render.ScreenBrightness` reaches the gamma update routine.
        // `cfg.render` is the profile `Config::apply_preferences` already read, so this is the
        // same file and not a second read of it. Nothing is written back.
        renderer.set_gamma(cfg.render.screen_brightness);
        // Presentation setup: windowed is always immediate; logical full screen consults
        // the full-screen vsync preference. Borderless DXGI is this build's full-screen
        // modernization, but the presentation policy still follows the logical retail mode.
        // `false`: the client starts windowed whatever the preference says, and
        // `App::change_presentation` sets this again on the gameplay edge.
        renderer.set_presentation_sync(false, cfg.display.sync_to_refresh);
        Ok(Box::new(renderer))
    }

    /// Bring the device up on the backend this run selected,
    /// and if that fails, on the default.
    ///
    /// The selection is `--renderer`, else the `Renderer=` preference (both already resolved into
    /// [`Config::renderer`]), else the default — Vulkan on every platform
    /// A backend that is not in this build, or that cannot create a device on this
    /// machine, is **a logged line and a fall back to the default**, never a panic and never a
    /// start-up failure; only the default failing too is a `StartupError::Device`.
    #[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
    fn device_on_selected_backend(
        handles: Option<dereth_render::device::WindowHandles>,
        client_w: u32,
        client_h: u32,
        cfg: &Config,
    ) -> Result<crate::gpu::Renderer, StartupError> {
        use dereth_render::device::Backend;

        let fallback = Backend::default_backend().ok_or_else(|| StartupError::Device {
            cause: "graphics engine: this build compiled no backend".to_string(),
        })?;
        // `Config::renderer` is a `dereth_client_contract::RendererChoice` -- the name a player
        // asked for -- so that `config.rs` can live in `dereth-client-runtime`. This is the one
        // site that creates a device, and it is where the choice meets `Backend`: the
        // switch/preference first, then the default.
        let wanted = cfg.renderer.map(Backend::from).unwrap_or(fallback);

        match crate::gpu::Renderer::new_on(wanted, handles, client_w, client_h) {
            Ok(r) => {
                tracing::info!("graphics backend {} on {}", wanted.name(), r.adapter_name());
                Ok(r)
            }
            Err(first) if wanted != fallback => {
                tracing::warn!(
                    "the {} backend is unavailable ({first}); \
                     falling back to {}",
                    wanted.name(),
                    fallback.name()
                );
                let r = crate::gpu::Renderer::new_on(fallback, handles, client_w, client_h)
                    .map_err(|e| StartupError::Device {
                        cause: format!("graphics engine: {e}"),
                    })?;
                tracing::info!(
                    "graphics backend {} on {}",
                    fallback.name(),
                    r.adapter_name()
                );
                Ok(r)
            }
            Err(e) => Err(StartupError::Device {
                cause: format!("graphics engine: {e}"),
            }),
        }
    }

    /// Decode one retail surface and stand it up as the scene.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the object is missing or will not decode.
    pub fn load_first_pixel_scene(&mut self) -> Result<(), StartupError> {
        let id = DataId(crate::gpu::FIRST_PIXEL_SURFACE);
        let assets: &dyn AssetSource = &*self.core.store;
        self.core
            .present
            .load_first_pixel_scene(assets, id)
            .map_err(|e| StartupError::Device {
                cause: format!("{e}"),
            })
    }

    /// `--ui-mode`: queue one of the eight modes by hand, for looking at a screen the flow would
    /// only reach through the network.
    pub fn queue_ui_mode(&mut self, m: dereth_ui::UiMode) {
        if let Some(shell) = self.shell.ui.as_mut() {
            shell.queue(m);
        }
    }

    /// The UI-flow shell, for the tests and for the report line.
    #[must_use]
    pub fn ui(&self) -> Option<&crate::ui::UiShell> {
        self.shell.ui.as_ref()
    }

    /// The UI-flow shell, mutably, so a test can drive a documented transition.
    pub fn ui_mut(&mut self) -> Option<&mut crate::ui::UiShell> {
        self.shell.ui.as_mut()
    }

    /// The three halves `Hud::drive` holds at once — the element tree, the panel holder and a live
    /// [`dereth_ui_screens::view::GameView`] — handed to one closure.
    ///
    /// It exists for the same reason [`App::apply_hud_events`] does and says so: the fields
    /// **cannot be borrowed through `App` from outside**, because `ui`, `hud` and `objects` are
    /// three private fields and an accessor per field hands out three conflicting borrows of
    /// `self`. Every panel handler in `dereth-ui-screens` takes `(&mut UiSystem, …, &dyn GameView)`,
    /// so a test that wants to enter one at all needs this shape once.
    ///
    /// The panels are moved out for the duration and put back after, which is exactly what
    /// `Hud::drive` does internally and for the same reason. `None` when no UI shell is up.
    pub fn with_panels<R>(
        &mut self,
        f: impl FnOnce(
            &mut dereth_ui::UiSystem,
            &mut dereth_ui_screens::panels::remaining::RemainingPanels,
            &dyn dereth_ui_screens::view::GameView,
        ) -> R,
    ) -> Option<R> {
        let shell = self.shell.ui.as_mut()?;
        let mut panels = std::mem::take(&mut self.core.hud.panels);
        let out = f(
            &mut shell.ui,
            &mut panels,
            &self.core.hud.view(&self.core.objects),
        );
        self.core.hud.panels = panels;
        Some(out)
    }

    /// What the key-binding page's three host drains have done, with denominators.
    #[must_use]
    pub fn key_binding_stats(&self) -> KeyBindingStats {
        self.shell.key_binding_stats
    }

    /// This frame's 2D blit list, for the tests.
    #[must_use]
    pub fn ui_draw_list(&self) -> &[dereth_ui::UiDrawCmd] {
        &self.shell.ui_draw_list
    }

    /// What the cursor path did this session — [`crate::cursor::CursorStats`].
    ///
    /// `updates` is the per-frame wire: it must equal the number of frames drawn.
    #[must_use]
    pub fn cursor_stats(&self) -> crate::cursor::CursorStats {
        self.shell.cursor.stats
    }

    /// The cursor asset currently applied, exposed for tests.
    #[must_use]
    pub fn current_cursor_did(&self) -> Option<dereth_primitives::DataId> {
        self.shell.cursor.current()
    }

    /// Access to the renderer, for the capture the acceptance gate takes.
    ///
    /// `App` holds a [`crate::present::Presentation`], so this is a downcast of it and is gated on the device feature exactly as the renderer itself is. It panics if the
    /// presentation is not the device one, which is the same contract the field access had: a
    /// caller of this function has already decided it is driving a real device.
    #[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
    pub fn renderer_mut(&mut self) -> &mut crate::gpu::Renderer {
        self.core
            .present
            .as_any_mut()
            .downcast_mut::<crate::gpu::Renderer>()
            .expect("App::renderer_mut on a presentation that is not the device")
    }

    /// What the last char-gen dressing tail reached: how many of the
    /// `ObjDesc` merges, the four calls and the three `Subpalette`s
    /// ran. A descriptor can legitimately be empty (a wizard with no heritage yet); this is how a
    /// test tells that apart from a block that never ran.
    #[must_use]
    pub fn chargen_dress(&self) -> crate::preview::ChargenDressStats {
        self.shell.chargen_dress
    }

    /// What the UI-texture releases have done, for the descriptor-bound assertion.
    #[must_use]
    pub fn ui_release_report(&self) -> crate::gpu::UiReleaseReport {
        self.shell.ui_release
    }

    /// Read-only access to the renderer, for the counters the report prints.
    ///
    /// See [`App::renderer_mut`] for why this is a downcast and what it asserts.
    #[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
    #[must_use]
    pub fn renderer(&self) -> &crate::gpu::Renderer {
        self.core
            .present
            .as_any()
            .downcast_ref::<crate::gpu::Renderer>()
            .expect("App::renderer on a presentation that is not the device")
    }

    /// The scene as one view: this `App`'s world state beside the renderer's drawing
    /// half, reading as a whole `WorldScene` did. Gated and asserting like [`App::renderer`].
    #[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
    #[must_use]
    pub fn world_scene(&self) -> Option<crate::world::WorldSceneRef<'_>> {
        let draw = self.renderer().world()?;
        Some(crate::world::WorldSceneRef {
            world: self.core.world.as_ref()?,
            draw,
        })
    }

    /// …and writable, both halves.
    #[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
    pub fn world_scene_mut(&mut self) -> Option<crate::world::WorldSceneMut<'_>> {
        let draw = self
            .core
            .present
            .as_any_mut()
            .downcast_mut::<crate::gpu::Renderer>()
            .expect("App::world_scene_mut on a presentation that is not the device")
            .world_mut()?;
        Some(crate::world::WorldSceneMut {
            world: self.core.world.as_mut()?,
            draw,
        })
    }
}

/// The UI's steps, with the application and its front end both in hand.
struct Ui<'a, H: Host> {
    core: &'a mut CoreApp<H>,
    shell: &'a mut ClientShell<H>,
}

impl<H: Host> Ui<'_, H> {
    /// The UI's own step of frame step 7.
    fn ui_frame(&mut self, now: dereth_primitives::LocalTime, notices: UiNotices) {
        let UiNotices {
            power_bar: power_bar_notices,
            external_container: external_container_notices,
            slumlord_range_exits,
            book_range_exits,
            salvage: salvage_notices,
            trade_for_dummies,
        } = notices;
        let shell = self
            .shell
            .ui
            .as_mut()
            .expect("the UI's own step runs only with a UI");
        let host = self.core.host_state.clone();
        // A mode transition replaces the framework even for the same mode id.
        // Count actual reconstructions, not UiShell's mode-id changes: the old HUD handles
        // and notice subscribers die on either transition.
        let switches_before = shell.flow.switches;
        // **Hide the UI on the way out of the gameplay mode.**
        //
        // The journal visibility handler saves the current page and then all pages; this is the
        // **only** path that writes the
        // journal file; journal-panel teardown does not save, and there is no timer. On a log
        // off retail reaches it from the teardown, which shows the old
        // framework false before it destroys it, and that hide propagates to every
        // child exactly as closing the tab by hand does.
        //
        // In this build `JournalPanel` lives on `Hud` and **outlives the screen**, so by the time
        // `JournalPanel::load` notices the next character's journal path, the edit boxes it
        // would have saved are gone. Without this the page the player was on when they logged off
        // is lost, and a relog into the same character shows the last *closed* page instead.
        //
        // `queued_mode()` is consumed by the flow update inside `frame_with_dispatch` below, so
        // this runs at most once per switch, and a mode transition replaces the framework even for the
        // same mode id — which is why the queued mode is not compared against the current one.
        // `on_visibility_changed` is a no-op while the screen is not loaded, so a queue before the
        // player is in the world writes nothing.
        if shell.flow.queued_mode().is_some()
            && shell.flow.current_mode() == Some(dereth_ui::framework::mode::GAME_PLAY)
        {
            self.core
                .hud
                .panels
                .journal
                .on_visibility_changed(&mut shell.ui, false);
        }
        let interaction = &mut self.core.interaction;
        let objects = &mut self.core.objects;
        let hud = &mut self.core.hud;
        let targeted_dialogs = &mut self.shell.targeted_dialogs;
        let audio = &mut self.core.audio;
        let viewport = self.core.present.size();
        // The FPS meter reads the process render globals during the
        // UI tick. This scene owns their reconstructed values; sample before the world update
        // later in the frame, matching native's UI-before- ordering.
        let framerate_display_values =
            self.core
                .present
                .scene(self.core.world.as_ref())
                .map(|world| {
                    let (auto_update_deg_mul, deg_mul, user_bias) = world.degrade_meter();
                    (
                        world.frame_rate_fps(),
                        dereth_ui_screens::hud::world_view::deg_var(
                            auto_update_deg_mul,
                            deg_mul,
                            user_bias,
                        ),
                    )
                });
        let present = &mut *self.core.present;
        let world = &mut self.core.world;
        let serial = self.shell.gameplay_serial;
        // Object search is gated on the input-device manager: without one, the client does not
        // search at all. A headless App with no `input_manager` runs `NullInputPump`, which
        // reproduces that condition.
        let cidm = self.shell.input.is_some();
        let mut early_unowned = Vec::new();
        let body = world.as_ref().and_then(|w| w.character.as_ref());
        interaction.prepare_ui_dispatch(body, &mut objects.world, viewport);
        let body = world.as_ref().and_then(|w| w.character.as_ref());
        interaction.dispatch_ui_selection_notices(body, objects, now);
        // Tell-to-Selected reads the live selected id and its name when it starts. `Hud::drive`
        // normally projects that pair at the end of this UI step, which is one frame too late when an
        // object was cleared after the preceding projection. Refresh just that existing snapshot
        // before input dispatch; the ordinary Hud pass below still owns the rest of the screen.
        if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
            crate::hud_drive::game_call(
                &mut shell.ui,
                screen,
                dereth_ui_screens::screens::gameplay_host::GameCall::AutoTargetWorld(
                    hud.auto_target_world(&objects.world),
                ),
            );
        }
        // Retail external listeners call game globals synchronously. UiSystem has released
        // its element callback borrow here, so the complete listener sequence can finish before
        // the next action. No selective Use extraction may overtake a drag lock or split write.
        let mut dispatch = |shell: &mut crate::ui::UiShell, point: crate::ui::UiDispatch| {
            if let crate::ui::UiDispatch::Mouse(event) = point {
                interaction.dispatch_ui_mouse(event, viewport);
                return;
            }
            // The smart-box global-loop listener, reached by
            // the global-message-3 broadcast. `UiShell::route_input` raises this hook once a
            // frame whether or not the pointer moved, and before the action drain, because that
            // is where the broadcast sits: after the mouse update and tooltip check and before
            // the input manager's per-frame update. The object finder takes the exact last-over
            // element as an item slot first; an empty or spell slot supplies id zero, while a
            // non-item falls through to the geometric pick.
            if let crate::ui::UiDispatch::Hover(position) = point {
                // The selection-blink flip counter in the global loop runs *before*
                // the input-device-manager gate, so the
                // blink ticks whether or not there is an input device to search with.
                if shell.flow.current_mode() == Some(dereth_ui::framework::mode::GAME_PLAY) {
                    interaction.global_loop_lighting(now.0);
                }
                if cidm && shell.flow.current_mode() == Some(dereth_ui::framework::mode::GAME_PLAY)
                {
                    let over = shell.ui.mouse_over();
                    let item = over
                        .and_then(|h| dereth_ui_screens::items::runtime::identity(&shell.ui, h))
                        .map(|i| i.0);
                    // `find_object`'s rectangle test belongs to *this* point.
                    // `note_game_viewport` also pushes this latch, once a frame and from the
                    // pointer position the previous frame left behind, so without this line the
                    // first hover after the pointer entered `<SBOX>` would be refused as being under
                    // a HUD window and armed no geometric pick. The last-entered element is what
                    // the object finder reads, and `UiSystem::mouse_over` is that field,
                    // already switched for this position by `UiShell::route_input`.
                    interaction.note_pointer_over_game_view(pointer_over_game_view_at(shell, over));
                    interaction.dispatch_ui_hover(
                        position,
                        item,
                        viewport,
                        &mut objects.world,
                        dereth_primitives::ServerTime(now.0),
                    );
                }
                return;
            }
            early_unowned.extend(dispatch_ui_owner_requests(
                shell,
                interaction,
                objects,
                hud,
                targeted_dialogs,
                audio.as_mut(),
                &mut *present,
                world,
                serial,
                now,
            ));
        };
        let mut requests = match self.shell.input.as_mut() {
            Some(input) => shell.frame_with_dispatch(now, &host, input, &mut dispatch),
            None => {
                shell.frame_with_dispatch(now, &host, &mut dereth_ui::NullInputPump, &mut dispatch)
            }
        };
        // `@title` raises the real popup notice after command dispatch. Its receiver draws the
        // literal and emits the set-chat-window-title notice; take that one local-module write back
        // into this frame's request list so PlayerModule persistence cannot lag behind the visible
        // caption.
        let chat_window_title_notices = interaction.take_chat_window_title_notices();
        if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
            for (window_id, title) in chat_window_title_notices {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    screen,
                    dereth_ui_screens::screens::gameplay_host::GameCall::ChatWindowTitle {
                        window_id,
                        title: dereth_ui_screens::view::ChatWindowTitle::Literal(title),
                    },
                );
            }
        }
        requests.extend(shell.ui.requests.take_placement_updates());

        // The frame-rate command has raised its ordered notice edge(s). Native notice handlers
        // have no replay history: drain them even when this is not a gameplay screen, and never
        // infer an enable for a replacement screen from the process flag. A bound receiver then
        // performs its ordinary guarded per-frame update from the existing renderer values.
        let framerate_display_notices = interaction.take_framerate_display_notices();
        let had_framerate_display_notice = !framerate_display_notices.is_empty();
        if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
            use dereth_ui_screens::screens::gameplay_host::GameCall;
            for enabled in framerate_display_notices {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    screen,
                    GameCall::FramerateDisplay {
                        enabled,
                        values: framerate_display_values,
                    },
                );
            }
            if !had_framerate_display_notice {
                if let Some((framerate, degrade)) = framerate_display_values {
                    crate::hud_drive::game_call(
                        &mut shell.ui,
                        screen,
                        GameCall::FramerateUseTime { framerate, degrade },
                    );
                }
            }
        }
        // Resolve each pending object and apply or restore its lighting, for every
        // call requested by the wrapper's notice and
        // the global loop made this frame — in this frame, before the world draws, as the
        // client's synchronous calls are. The object may be gone (the lookup returns nothing);
        // that is `apply_object_lighting`'s `false` and is dropped exactly as retail drops it.
        for (id, mode) in interaction.take_pending_lighting() {
            if let Some(mut scene) = self.core.present.scene_mut(self.core.world.as_mut()) {
                scene.world_mut().apply_object_lighting(id, mode);
            }
        }
        // These have no game owner. Preserve their existing diagnostic path, without replaying
        // any already completed request or silently dropping unsupported calls.
        requests.extend(early_unowned);
        let screen_changed = shell.flow.switches != switches_before;
        if screen_changed {
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                // Post-initialization selects row 0, then handles selection 1. Current global
                // enables initialize the new rows, but old notice history never does. Place
                // the focus write AFTER outgoing-screen requests, whose commands preceded it.
                let chat = &mut self.core.objects.world.chat;
                chat.set_talk_focus_enabled(dereth_client_model::chat::TalkFocus::Selected, false);
                // Selecting the row raises its own enable notice, which belongs to this `post_init`,
                // not a later input frame. Finish it before the explicit selection tail below.
                let notices = chat.take_talk_focus_notices();
                crate::hud::deliver_chat_focus_notices(&mut shell.ui, screen, chat, notices);
                if let dereth_ui_screens::screens::gameplay_host::GameCall::InitCommunication {
                    change: Some(change),
                    ..
                } = crate::hud_drive::game_call(
                    &mut shell.ui,
                    screen,
                    dereth_ui_screens::screens::gameplay_host::GameCall::InitCommunication {
                        enabled: chat.enabled_focuses(),
                        is_olthoi: self.core.hud.is_olthoi(&self.core.objects.world),
                        change: None,
                    },
                ) {
                    requests.push(dereth_client_contract::UiRequest::SetTalkFocus {
                        focus: change.focus,
                    });
                }
            }
        }
        // Old-screen/input placement calls completed before the new screen's PM restoration.
        // Consume these local writes now, not in the later network/interaction step.
        self.core.hud.consume_placement_requests(
            &mut self.core.objects.world,
            &mut requests,
            dereth_primitives::ServerTime(now.0),
        );
        let has_external_subscriber = shell.flow.current().is_some_and(|s| s.is_game());
        if screen_changed || !has_external_subscriber {
            self.core
                .objects
                .world
                .object_range_checks
                .unregister_all(dereth_client_model::range::RangeHandler::ExternalContainer);
        }
        // **The journal-path builder's three inputs.** The same
        // three the screen-layout path below is built from, and this is the one place in the
        // client that holds all of them: the preferences file is `--prefs`, the world name comes
        // from `0xF7E1 Login_WorldInfo` and the character is recorded on the log-on edge by
        // `run_character_actions`. Recomposed each frame rather than latched,
        // because the character changes with every log-on and the journal is per character.
        // `None` while any of the three is missing, which is a build that writes no journal at
        // all rather than one that writes to a guessed path.
        self.core.hud.journal_identity = (|| {
            let dir = self.core.cfg.preferences_file.parent()?;
            let world = self
                .core
                .host_state
                .world_name
                .as_deref()
                .filter(|s| !s.is_empty())?;
            let character = self
                .core
                .host_state
                .entered_character
                .as_deref()
                .filter(|s| !s.is_empty())?;
            Some(dereth_ui_screens::panels::journal::JournalIdentity {
                directory: dir.to_path_buf(),
                world: world.to_owned(),
                character: character.to_owned(),
            })
        })();
        // Retail adds `(msg, 0x1A, TRUE, 0)` to the text scroll, the
        // one complaint the journal's page load makes. The panel cannot reach the scroll from
        // `dereth-ui-screens`, so it records the line and this drains it.
        if let Some(text) = self.core.hud.panels.journal.take_load_complaint() {
            self.core.objects.world.scroll.add_text_to_scroll(
                text,
                dereth_client_model::scroll::LOCAL_ERROR_TYPE,
                true,
                0,
            );
        }
        // **The load end of the screen-layout file, and the only automatic one the client has.**
        // Player-description delivery loads the `"#auto"` screen layout and records whether the
        // layout came from a file; this is the caller of `dereth_ui_screens`' `load_screen_layout`
        // and the path builders.
        //
        // It is deferred to a frame rather than done in the `PlayerDescription` arm because the
        // mode switch is *queued*: `0x0013` arrives before the mode transition has built the gameplay
        // screen, and a notice that reaches no gameplay screen does nothing in the client either.
        // `None` is "not up yet, ask again"; a parse failure is reported and the flag cleared, so
        // a corrupt file cannot re-fail once per frame for the rest of the session.
        // The gameplay constructor's forced display resolution `(false, 800, 600)` is modeled at the
        // screen-lifetime edge below, after this shell borrow ends. On the construction frame the
        // new root therefore still measures the forced login presentation. `#auto` includes that
        // measurement in its filename, so looking now searches for `...-600-800.txt` and clears
        // the notice before the saved gameplay-size file can be found. Retail runs the constructor
        // (and lifts the force) before this notice receiver. Defer only this screen-change frame;
        // the next frame sees the restored presentation and preserves ordinary in-game 0x0013
        // refresh behavior.
        if self.core.pending_auto_layout && !screen_changed {
            let prefs = self.core.cfg.preferences_file.clone();
            let character = self
                .core
                .host_state
                .entered_character
                .clone()
                .unwrap_or_default();
            let world = self.core.host_state.world_name.clone().unwrap_or_default();
            if let Some(result) = shell.auto_load_screen_layout(&prefs, &character, &world) {
                self.core.pending_auto_layout = false;
                match result {
                    Ok(true) => {
                        tracing::info!("screen layout loaded for {character:?} on {world:?}")
                    }
                    Ok(false) => tracing::info!(
                        "no saved screen layout for {character:?} on {world:?} \
                         (layout not from file)"
                    ),
                    Err(e) => tracing::warn!("screen layout will not load: {e}"),
                }
            }
        }
        // Local layout loading also invokes TBAR's real resize/move tails. Commit their
        // achieved values before PM readback; leave other queued UI actions in place.
        self.core.hud.consume_placement_requests(
            &mut self.core.objects.world,
            &mut shell.ui.requests.take_placement_updates(),
            dereth_primitives::ServerTime(now.0),
        );
        // What the character-management screen asked the player system for. This is the whole of
        // the character screen's outbound half — log on, delete and restore a character — and it
        // is what makes clicking a character enter the world.
        let character_actions = shell.take_character_actions();
        let chargen_actions = shell.take_chargen_actions();
        // The epilogue screen logs off the character when the player system exists and
        // the network is still up.
        let log_off = shell.take_log_off();
        // Interaction takes what it owns out of this list in `interaction::use_time`; whatever it
        // hands back is still reported, because a request nobody owns must be a line and not a
        // silence.
        let mut ui_requests = requests;
        let mouse_events = shell.take_mouse_events();
        if shell.stats.device_done {
            // Mark the device done -- the epilogue screen's only job.
            self.core.pump.done();
        }
        // The HUD: the toolbar read-out, the radar's coordinates and compass, the player-module refresh and the chat deliveries — all of them inside step 7,
        // and all of them **before** the blit list is taken, or the frame would draw last frame's
        // HUD over this frame's world.
        if screen_changed {
            self.shell.gameplay_serial += 1;
        }
        let viewer = self
            .core
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(|c| {
                let p = c.position();
                crate::hud::ViewerFrame {
                    position: p,
                    heading_degrees: dereth_animation::frame::get_heading(&p.frame),
                }
            });
        self.core.hud.sync(&self.core.objects, viewer);
        // The map panel's first update block, whose source is the same
        // `GameTime` the sky runs on. Read beside the `ViewerFrame` above because this is the one
        // point in the frame where the scene and the HUD are both in hand; `Hud::sync` cannot do it
        // because it is not given the scene.
        self.core.hud.game_date_time = self
            .core
            .present
            .scene(self.core.world.as_ref())
            .and_then(|s| s.game_date_time());
        {
            let serial = self.shell.gameplay_serial;
            let objects = &self.core.objects;
            let hud = &mut self.core.hud;
            let shell = self.shell.ui.as_mut().expect("checked above");
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                if !screen_changed {
                    hud.pending_external_container
                        .extend(external_container_notices);
                    hud.queue_salvage_notices(salvage_notices);
                    for slumlord in slumlord_range_exits {
                        hud.panels
                            .slumlord
                            .recv_object_range_exit(&mut shell.ui, slumlord);
                    }
                    use dereth_ui_screens::screens::gameplay_host::GameCall;
                    for book in book_range_exits {
                        crate::hud_drive::game_call(
                            &mut shell.ui,
                            screen,
                            GameCall::BookRangeExit(book),
                        );
                    }
                    // Into the same one-frame queue a drop on the table uses, so the optimistic row
                    // and the `0x01F8` come from `TradePanel::drop_item` once, from both of
                    // retail's two callers.
                    for item in trade_for_dummies {
                        crate::hud_drive::game_call(
                            &mut shell.ui,
                            screen,
                            GameCall::OfferTradeItem(item),
                        );
                    }
                }
                hud.drive(&mut shell.ui, screen, serial, objects);
                // A newly constructed subscriber did not exist when this batch was emitted.
                // Its `post_init` state remains authoritative until a subsequent notice arrives.
                if !screen_changed {
                    let writes = u64::from(crate::hud::deliver_power_bar_notices(
                        &mut shell.ui,
                        &mut hud.panels,
                        power_bar_notices,
                    ));
                    hud.stats.power_bar_writes += writes;
                    hud.stats.panels_written += writes;
                }
            }
        }
        self.drive_key_bindings();
        // Save to the keymap-file path (not forced), the *OK*
        // button's first line — the input manager saves the keymap to `dir + name`. Here rather
        // than in `UiShell` because the writer is `InputShell::save_keymap`, the same one shutdown
        // uses; the shell only latches the request.
        if self
            .shell
            .ui
            .as_mut()
            .is_some_and(crate::ui::UiShell::take_save_keymap)
        {
            match self
                .shell
                .input
                .as_ref()
                .map(crate::input::InputShell::save_keymap)
            {
                Some(Ok(true)) => {}
                Some(Ok(false)) | None => {
                    tracing::warn!("the key bindings were applied with no keymap path");
                }
                Some(Err(e)) => tracing::warn!("the keymap was not saved: {e}"),
            }
        }
        // PM restoration/this frame's HUD writes are synchronous local property changes in
        // retail. Do not defer them to next frame or make their cache change trigger a re-seed.
        self.core.hud.consume_placement_requests(
            &mut self.core.objects.world,
            &mut self
                .shell
                .ui
                .as_mut()
                .map(|s| s.ui.requests.take_placement_updates())
                .unwrap_or_default(),
            dereth_primitives::ServerTime(now.0),
        );
        // Hud's external panel updates call game globals too: e.g.
        // selects the first row synchronously. Finish those calls in their producing frame,
        // not as retained requests which overwrite the next frame's fresh selection/input.
        // OpenVendor first emits an actual Menu message; its external handler then produces
        // Select. Draining only direct requests would leave that broadcast one frame late.
        let present = &mut *self.core.present;
        let world = &mut self.core.world;
        let inter = &mut self.core.interaction;
        let objects = &mut self.core.objects;
        let hud = &mut self.core.hud;
        let targeted_dialogs = &mut self.shell.targeted_dialogs;
        let audio = &mut self.core.audio;
        let serial = self.shell.gameplay_serial;
        self.shell
            .ui
            .as_mut()
            .expect("checked above")
            .deliver_with_dispatch(&mut |shell, _| {
                ui_requests.extend(dispatch_ui_owner_requests(
                    shell,
                    inter,
                    objects,
                    hud,
                    targeted_dialogs,
                    audio.as_mut(),
                    &mut *present,
                    world,
                    serial,
                    now,
                ));
            });
        let shell = self.shell.ui.as_mut().expect("checked above");
        // The movie's current frame, uploaded outside the frame bracket with the rest
        // of the UI textures and released the moment the next one replaces it.
        let movie_frame = shell.take_movie_frame();
        // The same movie's soundtrack. It is taken beside the frame because the two come from one movie open and the audio
        // renderer starts with the video one.
        let movie_audio_cue = shell.take_movie_audio_cue();
        // The media-playback sound update — every sound media step this frame's element state
        // changes ran, which is every button click sound in the game. Taken here, beside the
        // movie's cue, because it is the same hand-off for the same reason and because the client
        // makes the call synchronously: a click's sound must start on the frame of the click.
        let ui_sound_requests = shell.take_sound_requests();
        // After the shell borrow ends: these reach the session, which the shell does not own.
        self.core.run_character_actions(character_actions);
        self.core.run_chargen_actions(chargen_actions);
        if log_off {
            self.core.log_off_character();
        }
        // The mode transition destroyed the outgoing screen and every element in it, so every
        // image-texture link it held is gone: release the descriptor slots before the incoming screen
        // asks for its own. Here rather than inside the frame bracket, so a slot freed now is
        // reusable by the very next upload instead of waiting on this frame's fence.
        if screen_changed {
            let r = self.core.present.release_ui_textures();
            self.shell.ui_release.freed += r.freed;
            self.shell.ui_release.still_linked += r.still_linked;
            self.shell.ui_release.unknown += r.unknown;
            // Gameplay-screen construction and teardown, which are the only two
            // screen-lifetime calls to display-resolution forcing. It runs here for the
            // same reason as the texture release above: the flow has just destroyed
            // the outgoing screen and built the incoming one, so this *is* the ctor/dtor edge.
            self.core.follow_screen_forced_resolution(self.shell);
            self.core.follow_gameplay_full_screen(self.shell);
        }
        if let Some(frame) = movie_frame {
            self.core
                .present
                .set_movie_frame(crate::ui::MOVIE_IMAGE_ID, &frame);
        }
        if let (Some(cue), Some(audio)) = (movie_audio_cue, self.core.audio.as_mut()) {
            match cue {
                crate::ui::MovieAudioCue::Start(a) => audio.play_movie_audio(&a),
                crate::ui::MovieAudioCue::Stop => audio.stop_movie_audio(),
            }
        }
        // Anything that is not a `PlaySound` comes straight back out and joins the
        // frame's other unowned requests, so nothing disappears into the sound system.
        let leftover = crate::audio::apply_sound_requests(
            self.core.audio.as_mut(),
            &self.core.store,
            ui_sound_requests,
        );
        for r in leftover {
            tracing::warn!("UI sound drain returned {r:?}, which cannot happen");
        }
        // The 3D character preview's update, then the preview pass's own device work -- both
        // **outside** the frame bracket, for the same reason `prepare_ui` is: adding preview objects uploads
        // textures and `Gpu::upload_texture` runs a command list of its own.
        self.preview_use_time();
        // What the pointer and the screens asked for, acted on later in the frame where interaction runs.
        self.core.interaction.queue(mouse_events, ui_requests);
        // The teleport overlay is one of the UI elements
        // the shell ticks, so it belongs to this step rather than the world-object step.
        self.core.teleport_use_time(self.shell);
        // The portal-space view advances from inside its
        // own UI tick, so this is the same step and runs **after** the state machine: the tunnel
        // state it reads is this frame's. The frame counter it hands back is read by the *next*
        // tick's tunnel-continue arm, a lag of one frame at 60 Hz against a window 200 ms
        // wide. Still outside `begin_frame`/`end_frame`: adding preview objects uploads textures.
        self.portal_space_use_time();
        // The paper-doll panel is a UI element too, and its space is built by the same
        // viewport-element machinery -- so it belongs in this step for the same reason and
        // outside the frame bracket for the same reason.
        self.paper_doll_use_time();
        // The identify portrait is a third viewport element and
        // belongs in the same step for the same two reasons: its space is queued per frame from
        // the element's own draw, and adding a preview object uploads textures so it must stay outside
        // `begin_frame`/`end_frame`.
        self.examine_3d_use_time();
    }

    /// Bring the UI up, after input and sound.
    fn start_ui(&mut self) -> Result<(), StartupError> {
        // Initialize the UI after input and sound.
        if self.core.cfg.ui {
            let (w, h) = self.core.present.size();
            let display = (
                i32::try_from(w).unwrap_or(i32::MAX),
                i32::try_from(h).unwrap_or(i32::MAX),
            );
            let mut shell = crate::ui::UiShell::new(&self.core.store, display).map_err(|e| {
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
            self.core.register_display_modes();
            match dereth_client_runtime::platform::files::read_to_string(
                &self.core.cfg.preferences_file,
            )
            .ok()
            .and_then(|t| {
                dereth_client_contract::persist::preferences::UserPreferences::parse(&t).ok()
            }) {
                Some(ini) => {
                    let (applied, ignored) = dereth_client_contract::options::store::load(&ini);
                    tracing::debug!(
                        "user preferences: {applied} applied, {ignored} for other owners"
                    );
                }
                // "A missing file is not an error": retail ignores the preference loader's
                // result, and the registered defaults are then what the page shows.
                None => {
                    tracing::info!("no UserPreferences.ini; registered defaults stand")
                }
            }
            // `--object-visuals` wins over the profile's object mode, and the options page shows
            // what is drawn.
            if let Some(style) = self.core.cfg.object_visuals {
                let _ = dereth_client_contract::options::store::set_value(
                    dereth_client_contract::options::landscape::OBJECTS,
                    dereth_client_contract::PrefValue::Int(style.map_or(
                        dereth_client_contract::options::landscape::WORLD_DEFAULT,
                        dereth_client_contract::options::landscape::RegionStyle::value,
                    )),
                );
            }
            crate::render_prefs::seed_ui_registry(self.core.present.texture_filtering());
            // The data-movie loader's no-database-file path is "a plain file path, resolved
            // relative to the working directory". The client is started from its install directory;
            // this build takes the directory the dats came from, which is the same place.
            shell.client_dir = self.core.cfg.dat_dir.clone();
            tracing::info!(
                "UI up, {} mode(s) registered, {:?} queued",
                dereth_ui::framework::mode::REGISTRATION_ORDER.len(),
                shell.flow.queued_mode()
            );
            self.shell.ui = Some(shell);
            // The UI comes up on a pre-game screen, where no character session exists yet, so the
            // session's maps go now rather than on the UI's first frame: a key pressed before that
            // frame must not reach them either.
            if let Some(input) = self.shell.input.as_mut() {
                input.set_character_session_input_maps(false);
            }
        }
        Ok(())
    }

    /// `--enter-world`, pressed into the pre-game screens rather than into the session.
    ///
    /// Every click here is an element-message broadcast on the element the layout
    /// actually carries, and every consequence is the screen's own: `EnterGame` for the
    /// double-click and the finish step for the wizard. Nothing touches the desktop, so this
    /// is usable as evidence in a way an injected keystroke is not.
    fn drive_pregame_screens(&mut self) {
        use dereth_ui::framework::mode;
        use dereth_ui::msg::element::id::BUTTON_CLICKED;
        use dereth_ui::{ElementId, MessageId};

        if !self.core.cfg.enter_world {
            return;
        }
        let wanted = if self.core.cfg.create_char.is_empty() {
            self.core.cfg.start_char.clone()
        } else {
            self.core.cfg.create_char.clone()
        };
        let creating = !self.core.cfg.create_char.is_empty();
        let create_char = self.core.cfg.create_char.clone();
        let script = self.core.script;
        let mut pregame = self.shell.pregame;
        let Some(shell) = self.shell.ui.as_mut() else {
            return;
        };
        match shell.flow.current_mode() {
            Some(mode::CHARACTER_MANAGEMENT) => {
                if pregame == PregameDrive::OpeningWizard
                    || script != EnterWorldScript::AwaitingCharacterSet
                {
                    return;
                }
                let root = match shell.flow.current() {
                    Some(s) if !s.roots().is_empty() => s.roots()[0],
                    _ => return,
                };
                // `-r`/`-create`: press *Create Character* (`0x100003A0`) rather than entering.
                if creating && pregame == PregameDrive::Idle {
                    if let Some(h) = shell.ui.get_child_recursive(root, ElementId(0x1000_03A0)) {
                        tracing::info!("pressing Create Character");
                        shell.ui.broadcast_element_message(h, BUTTON_CLICKED, 7, 0);
                        self.shell.pregame = PregameDrive::OpeningWizard;
                    }
                    return;
                }
                // Otherwise pick the row `-u` named, or the first, and double-click it.
                let row = {
                    use dereth_ui_screens::screens::pregame_host::PregameCall;
                    let Some(s) = shell.flow.current_mut() else {
                        return;
                    };
                    let (took, call) = crate::hud_drive::pregame_call(
                        &mut shell.ui,
                        &mut **s,
                        PregameCall::PickCharacterRow {
                            wanted: wanted.clone(),
                            out: None,
                        },
                    );
                    let PregameCall::PickCharacterRow { out, .. } = call else {
                        return;
                    };
                    if !took {
                        return;
                    }
                    out
                };
                let Some(row) = row else { return };
                tracing::info!("double-clicking a character row");
                shell
                    .ui
                    .broadcast_element_message(row, BUTTON_CLICKED, 7, 0);
                shell
                    .ui
                    .broadcast_element_message(row, MessageId(0x1A), 7, 0);
                pregame = PregameDrive::Entering;
            }
            Some(mode::CHAR_GEN) if pregame == PregameDrive::OpeningWizard => {
                use dereth_ui_screens::screens::chargen::{HERITAGE_BUTTONS, TOWN_BUTTONS};
                let root = match shell.flow.current() {
                    Some(s) if !s.roots().is_empty() => s.roots()[0],
                    _ => return,
                };
                // Aluvian and Holtburg: the wizard's first heritage and first town, and the two
                // that need no expansion. The scripted path does not visit the appearance and
                // profession pages, so the character takes the default template's six fifties.
                // Then the summary tab: the finish button answers only on the summary page.
                for id in [
                    HERITAGE_BUTTONS[0].0,
                    TOWN_BUTTONS[0].0,
                    dereth_ui_screens::screens::chargen::EcgProgress::Summary
                        .select_button()
                        .expect("the summary tab"),
                ] {
                    if let Some(h) = shell.ui.get_child_recursive(root, id) {
                        shell.ui.broadcast_element_message(h, BUTTON_CLICKED, 7, 0);
                    }
                }
                // The name and Finish are the next frame's: the summary page, once shown,
                // writes its own text into the name field.
                pregame = PregameDrive::Naming;
            }
            Some(mode::CHAR_GEN) if pregame == PregameDrive::Naming => {
                use dereth_ui_screens::screens::chargen::NAME_FIELD;
                let root = match shell.flow.current() {
                    Some(s) if !s.roots().is_empty() => s.roots()[0],
                    _ => return,
                };
                if let Some(h) = shell.ui.get_child_recursive(root, NAME_FIELD) {
                    if let Some(t) = shell.ui.text_element_mut(h) {
                        t.set_text(&create_char);
                    }
                    shell.ui.broadcast_element_message(h, MessageId(0x44), 0, 0);
                }
                if let Some(h) = shell.ui.get_child_recursive(root, ElementId(0x1000_03C8)) {
                    tracing::info!("creating {create_char:?} -- heritage 1, Holtburg");
                    shell.ui.broadcast_element_message(h, BUTTON_CLICKED, 7, 0);
                }
                pregame = PregameDrive::Finishing;
            }
            Some(mode::CHAR_GEN) if pregame == PregameDrive::Finishing => {
                use dereth_ui_screens::screens::pregame_host::PregameCall;
                // The finish step refuses while attribute credits are unspent, and the default
                // template always leaves 30 of the 330. Answering the warning is the player's
                // second click; here it is the same call the dialog-close handler makes.
                // It has to be a later frame than the Finish click: element messages are queued and
                // delivered inside `UiShell::frame`, so the dialog does not exist yet when the
                // click is broadcast.
                if let Some(w) = shell.flow.current_mut() {
                    let (_, open) = crate::hud_drive::pregame_call(
                        &mut shell.ui,
                        &mut **w,
                        PregameCall::CreditWarningOpen(false),
                    );
                    if matches!(open, PregameCall::CreditWarningOpen(true)) {
                        tracing::info!("answering the unspent-credit warning");
                        let (took, _) = crate::hud_drive::pregame_call(
                            &mut shell.ui,
                            &mut **w,
                            PregameCall::CloseDialog(true),
                        );
                        // The screen that just said its warning is open is the wizard, so it
                        // must take the answer too.
                        debug_assert!(took, "the creation wizard did not take the dialog answer");
                        pregame = PregameDrive::Creating;
                    }
                }
            }
            _ => {}
        }
        self.shell.pregame = pregame;
    }

    /// `--say`, submitted from the world screen: five seconds after arriving, then one line every
    /// two seconds, each as the chat window's Send submits it. A line is taken off the list as it
    /// goes, so it is sent once. Then each `--use` target, six seconds apart, used as a
    /// double-click uses it; `closest` is first selected by the closest-compass-item key's action
    /// and used a second later.
    fn drive_say(&mut self, now: f64) {
        use dereth_ui::framework::mode;

        let last = self.shell.say_drive;
        if self.core.cfg.say.is_empty() && self.core.cfg.use_targets.is_empty() {
            return;
        }
        let in_world = self.core.host_state.in_world;
        let Some(shell) = self.shell.ui.as_mut() else {
            return;
        };
        if !in_world || shell.flow.current_mode() != Some(mode::GAME_PLAY) {
            return;
        }
        let Some(last) = last else {
            self.shell.say_drive = Some(now + 3.0);
            return;
        };
        if self.core.cfg.say.is_empty() {
            let target = self.core.cfg.use_targets[0].clone();
            let id = target
                .strip_prefix("0x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .map(dereth_primitives::ObjectId);
            if let Some(id) = id {
                if now - last < 6.0 {
                    return;
                }
                self.core.cfg.use_targets.remove(0);
                tracing::info!("using {id:?}");
                shell
                    .ui
                    .requests
                    .emit(dereth_ui_screens::view::UiRequest::Use(id));
            } else if self.shell.say_selected {
                if now - last < 1.0 {
                    return;
                }
                self.shell.say_selected = false;
                self.core.cfg.use_targets.remove(0);
                if let Some(id) = self.core.objects.world.selected {
                    let name =
                        self.core.objects.world.weenie(id).map(|w| {
                            w.object_name(dereth_client_model::weenie::NameType::Appropriate)
                        });
                    tracing::info!("using the closest compass item {id:?} {name:?}");
                    shell
                        .ui
                        .requests
                        .emit(dereth_ui_screens::view::UiRequest::Use(id));
                }
            } else if now - last >= 6.0 {
                tracing::info!("selecting the closest compass item");
                self.core
                    .actions
                    .inject(dereth_client_contract::actions::Action {
                        id: dereth_client_contract::actions::ActionId(0x1000_002F),
                        phase: dereth_client_contract::actions::ActionPhase::Begin,
                        extent: 1.0,
                        repeats: 0,
                    });
                self.shell.say_selected = true;
            } else {
                return;
            }
            self.shell.say_drive = Some(now);
            return;
        }
        if now - last < 2.0 {
            return;
        }
        let text = self.core.cfg.say.remove(0);
        tracing::info!("saying {text:?} ({} more to say)", self.core.cfg.say.len());
        shell
            .ui
            .requests
            .emit(dereth_ui_screens::view::UiRequest::ChatLine { text, window: 0 });
        self.shell.say_drive = Some(now);
    }

    /// `--cast`, pressed into the world screen: the backpack button, the carried caster used
    /// (which wields it), the combat toggle (magic mode, with a caster in hand), a selection when
    /// the spell wants a target, the spell bar's cast, the toggle back to peace, and then a walk
    /// up to the closest compass item, used.
    ///
    /// The backpack is opened by the element message its toolbar button takes from a click, the
    /// caster by the use request a double-click raises, the mode by the toggle's action, and the
    /// cast is the request the spell bar's Cast button raises, so the client's own component
    /// check runs before anything goes to the server. The waits let the world settle: the
    /// inventory arrives with the player description and the body materializes after it.
    fn drive_world_script(&mut self, now: f64) {
        use dereth_ui::framework::mode;
        use dereth_ui::msg::element::id::BUTTON_CLICKED;
        use dereth_ui_screens::toolbar::INVENTORY_BUTTON;

        let Some(spell_id) = self.core.cfg.cast else {
            return;
        };
        let in_world = self.core.host_state.in_world;
        let drive = self.shell.world_drive;
        let Some(shell) = self.shell.ui.as_mut() else {
            return;
        };
        if !in_world || shell.flow.current_mode() != Some(mode::GAME_PLAY) {
            return;
        }
        self.shell.world_drive = match drive {
            WorldDrive::Idle => WorldDrive::Arrived(now),
            WorldDrive::Arrived(t) if now - t >= 5.0 => {
                let root = match shell.flow.current() {
                    Some(s) if !s.roots().is_empty() => s.roots()[0],
                    _ => return,
                };
                let Some(h) = shell.ui.get_child_recursive(root, INVENTORY_BUTTON) else {
                    return;
                };
                tracing::info!("opening the backpack");
                shell.ui.broadcast_element_message(h, BUTTON_CLICKED, 7, 0);
                WorldDrive::BackpackOpened(now)
            }
            WorldDrive::BackpackOpened(t) if now - t >= 2.0 => {
                // Spells are cast in magic mode, and magic mode wants a caster in hand.
                if let Some(caster) = scripted_caster(&self.core.objects.world) {
                    tracing::info!("wielding the caster {caster:?}");
                    shell
                        .ui
                        .requests
                        .emit(dereth_ui_screens::view::UiRequest::Use(caster));
                }
                WorldDrive::CasterWielded(now)
            }
            WorldDrive::CasterWielded(t) if now - t >= 3.0 => {
                tracing::info!("entering combat (magic) mode");
                self.core
                    .actions
                    .inject(dereth_client_contract::actions::Action {
                        id: dereth_client_contract::actions::ActionId(0x1000_005A),
                        phase: dereth_client_contract::actions::ActionPhase::Begin,
                        extent: 1.0,
                        repeats: 0,
                    });
                WorldDrive::MagicMode(now)
            }
            WorldDrive::MagicMode(t) if now - t >= 3.0 => {
                // A spell cast at another needs a selection, as a player's would: the player,
                // else the first worn or carried item the client's own target test accepts.
                if let Some(target) = scripted_cast_target(&self.core.objects.world, spell_id) {
                    tracing::info!("selecting {target:?} for spell {spell_id}");
                    shell
                        .ui
                        .requests
                        .emit(dereth_ui_screens::view::UiRequest::Select(target));
                }
                WorldDrive::TargetChosen(now)
            }
            WorldDrive::TargetChosen(t) if now - t >= 1.0 => {
                tracing::info!("casting spell {spell_id}");
                shell
                    .ui
                    .requests
                    .emit(dereth_ui_screens::view::UiRequest::CastSpell { spell_id });
                WorldDrive::Cast(now)
            }
            // Only once the cast is over (the server's use-done has come back): leaving magic
            // mode earlier breaks the cast off.
            WorldDrive::Cast(t)
                if now - t >= 2.0 && self.core.objects.world.magic.busy_count == 0 =>
            {
                tracing::info!("leaving combat (peace) mode");
                self.core
                    .actions
                    .inject(dereth_client_contract::actions::Action {
                        id: dereth_client_contract::actions::ActionId(0x1000_005A),
                        phase: dereth_client_contract::actions::ActionPhase::Begin,
                        extent: 1.0,
                        repeats: 0,
                    });
                WorldDrive::Peace(now)
            }
            WorldDrive::Peace(t) if now - t >= 3.0 => {
                // Then go up to the nearest person or thing, as the frame's close-up.
                tracing::info!("selecting the closest compass item");
                self.core
                    .actions
                    .inject(dereth_client_contract::actions::Action {
                        id: dereth_client_contract::actions::ActionId(0x1000_002F),
                        phase: dereth_client_contract::actions::ActionPhase::Begin,
                        extent: 1.0,
                        repeats: 0,
                    });
                WorldDrive::Approaching(now)
            }
            WorldDrive::Approaching(t) if now - t >= 1.0 => {
                let world = &self.core.objects.world;
                if let Some(id) = world.selected {
                    let name = world
                        .weenie(id)
                        .map(|w| w.object_name(dereth_client_model::weenie::NameType::Appropriate));
                    tracing::info!("walking up to {id:?} {name:?}");
                    shell
                        .ui
                        .requests
                        .emit(dereth_ui_screens::view::UiRequest::Use(id));
                }
                WorldDrive::Done
            }
            other => other,
        };
    }

    /// **The key-binding page's three host calls.**
    ///
    /// The Character Options drains that need only a `GameView` run in `Hud::drive`. These three
    /// need the host's `dereth_input::InputManager`, which `Hud::drive` is not given; without this
    /// function the page builds no rows in a running client and a captured key reaches nothing.
    ///
    /// The client's own three call sites, in this order:
    ///
    /// | this | client |
    /// |---|---|
    /// | `Self::key_bindings_built` gate | initialize options once per page |
    /// | the key-hit drain | call the registered key-hit handler inside input firing |
    /// | `drive_key_bindings` | handle the option-page element message synchronously |
    /// | `set_key_hit_handler` | register the row as input handler `0x20` when binding starts |
    ///
    /// **The order inside matters and is the client's.** Option initialization comes first, because nothing can
    /// capture before there are rows. The key hits **after** `drive_key_bindings` would lose a
    /// frame; before it would dispatch a key into a row whose capture this frame's click has not
    /// started yet. Queued hits therefore go before the drain, which consumes them. Key-hit
    /// handling runs inside the message pump, while element-message handling runs
    /// in the frame — the same two moments the client has.
    ///
    /// **The registration is mirrored at the end, on the edge**, exactly as
    /// `UiShell::sync_text_mode` mirrors: this crate cannot hand
    /// `dereth_input` a pointer to an action-key-map option, so the row's own *"is my map-warn
    /// dialog up"* state is copied into the manager's exclusive
    /// key-hit-handler registration instead.
    fn drive_key_bindings(&mut self) {
        use crate::hud_drive::game_call;
        use dereth_ui_screens::screens::gameplay_host::{GameCall, KeyBindingsCall as K};
        let serial = self.shell.gameplay_serial;
        let preferences_file = self.core.cfg.preferences_file.clone();
        let built = &mut self.shell.key_bindings_built;
        let stats = &mut self.shell.key_binding_stats;
        let (Some(shell), Some(input)) = (self.shell.ui.as_mut(), self.shell.input.as_mut()) else {
            return;
        };
        let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) else {
            return;
        };
        // One call on the page, with the input manager lent to it.
        let mut page =
            |ui: &mut dereth_ui::UiSystem, manager: &mut dereth_input::InputManager, k: K| -> K {
                let mut call = GameCall::KeyBindings(k);
                let took = screen.on_game(
                    &mut dereth_ui::framework::ScreenCx::new(ui),
                    &mut dereth_ui::framework::GameCx {
                        call: &mut call,
                        lent: None,
                        input: Some(manager),
                    },
                );
                crate::hud_drive::expect_taken(took, &call);
                match call {
                    GameCall::KeyBindings(k) => k,
                    _ => unreachable!("the page answers in the call it was given"),
                }
            };
        let _ = game_call;

        // 1., once per screen construction.
        if *built != Some(serial) {
            *built = Some(serial);
            let K::InitOptions {
                out: (rows, headers, failures),
            } = page(
                &mut shell.ui,
                &mut input.manager,
                K::InitOptions { out: (0, 0, 0) },
            )
            else {
                return;
            };
            let bindable = input
                .manager
                .action_map
                .entries()
                .filter(|(map, action, _)| input.manager.action_map.is_user_bindable(*map, *action))
                .count();
            stats.init_calls += 1;
            stats.rows_built = rows;
            stats.bindable_actions = bindable;
            stats.headers = headers;
            stats.failures = failures;
            let name = input.keymap_file_name();
            page(&mut shell.ui, &mut input.manager, K::RefreshFileName(name));
            // A denominator, not a bare number: `0` and `0 of 306` read the same in a log and only
            // one of them is a bug.
            tracing::debug!(
                "key bindings -- {rows} of {bindable} bindable action(s) got a row, \
                 {} section header(s), {} failure(s)",
                headers,
                failures
            );
        }

        // 2. -- the controls the manager diverted
        //    while a capture was in flight, offered to the page that asked for them.
        for control in input.take_key_hits() {
            stats.key_hits_offered += 1;
            if let K::KeyHit { taken: true, .. } = page(
                &mut shell.ui,
                &mut input.manager,
                K::KeyHit {
                    control,
                    taken: false,
                },
            ) {
                stats.key_hits_taken += 1;
            }
        }

        // 3. The per-frame drain: the queued element messages, then the queued key hits.
        let K::Drive { events, verdicts } = page(
            &mut shell.ui,
            &mut input.manager,
            K::Drive {
                events: Vec::new(),
                verdicts: Vec::new(),
            },
        ) else {
            return;
        };
        stats.row_events += events.len() as u64;
        stats.captures += verdicts.len() as u64;
        for v in &verdicts {
            if matches!(v, dereth_input::binding::Capture::Ready { .. }) {
                stats.bindings_made += 1;
            }
        }

        // 3b. The key-binding page's four button arms, drained from the same pass as the rows' —
        //     Apply, Cancel, *Restore Defaults* and *Revert to Saved*.
        let K::TakePageEvents(page_events) = page(
            &mut shell.ui,
            &mut input.manager,
            K::TakePageEvents(Vec::new()),
        ) else {
            return;
        };
        for e in page_events {
            use dereth_ui_screens::options::keybinding::PageEvent;
            stats.page_button_events += 1;
            match e {
                PageEvent::Applied { rows, saved } => {
                    stats.rows_applied += rows as u64;
                    if saved {
                        stats.keymaps_written += 1;
                    }
                }
                PageEvent::RestoredSaved(n) => stats.rows_reverted += n as u64,
                PageEvent::RestoredDefaults(n) => stats.rows_defaulted += n as u64,
                PageEvent::LoadKeymapDialog => match input.keymap_files() {
                    Ok(files) => {
                        let current = input.keymap_file_name();
                        page(
                            &mut shell.ui,
                            &mut input.manager,
                            K::OpenLoad { files, current },
                        );
                    }
                    Err(error) => tracing::warn!("enumerate keymaps failed: {error}"),
                },
                PageEvent::SaveKeymapDialog => {
                    page(&mut shell.ui, &mut input.manager, K::OpenSave);
                }
                PageEvent::LoadKeymap(name) => match input.load_keymap_file(&name) {
                    Ok(true) => {
                        if let Err(error) =
                            crate::input::save_keymap_preference(&preferences_file, &name)
                        {
                            tracing::warn!("save keymap preference failed: {error}");
                        }
                        page(&mut shell.ui, &mut input.manager, K::Reinit);
                        page(
                            &mut shell.ui,
                            &mut input.manager,
                            K::RefreshFileName(Some(name)),
                        );
                    }
                    Ok(false) => {}
                    Err(error) => tracing::warn!("load keymap failed: {error}"),
                },
                PageEvent::SaveKeymap(name) => match input.save_keymap_as(&name, false) {
                    Ok(Some(crate::input::SaveKeymapAs::Saved)) => {
                        stats.keymaps_written += 1;
                        if let Some(name) = input.keymap_file_name() {
                            if let Err(error) =
                                crate::input::save_keymap_preference(&preferences_file, &name)
                            {
                                tracing::warn!("save keymap preference failed: {error}");
                            }
                            page(
                                &mut shell.ui,
                                &mut input.manager,
                                K::RefreshFileName(Some(name)),
                            );
                        }
                    }
                    Ok(Some(crate::input::SaveKeymapAs::NeedsOverwrite)) => {
                        page(&mut shell.ui, &mut input.manager, K::OpenOverwrite(name));
                    }
                    Ok(Some(crate::input::SaveKeymapAs::ReadOnly)) => {
                        page(&mut shell.ui, &mut input.manager, K::OpenReadOnly(name));
                    }
                    Ok(None) => {}
                    Err(error) => tracing::warn!("save keymap failed: {error}"),
                },
                PageEvent::OverwriteKeymap(name) => match input.save_keymap_as(&name, true) {
                    Ok(Some(crate::input::SaveKeymapAs::Saved)) => {
                        stats.keymaps_written += 1;
                        if let Some(name) = input.keymap_file_name() {
                            if let Err(error) =
                                crate::input::save_keymap_preference(&preferences_file, &name)
                            {
                                tracing::warn!("save keymap preference failed: {error}");
                            }
                            page(
                                &mut shell.ui,
                                &mut input.manager,
                                K::RefreshFileName(Some(name)),
                            );
                        }
                    }
                    Ok(Some(crate::input::SaveKeymapAs::ReadOnly)) => {
                        page(&mut shell.ui, &mut input.manager, K::OpenReadOnly(name));
                    }
                    Ok(Some(crate::input::SaveKeymapAs::NeedsOverwrite)) | Ok(None) => {}
                    Err(error) => tracing::warn!("overwrite keymap failed: {error}"),
                },
            }
        }

        // 4. Register the row as input handler `0x20` / unregister it, on the edge.
        let K::Capturing(capturing) = page(&mut shell.ui, &mut input.manager, K::Capturing(false))
        else {
            return;
        };
        if capturing != input.manager.key_hit_handler_registered() {
            input.set_key_hit_handler(capturing);
        }
    }

    /// The normal-render target callback, after camera/selection updates.
    /// Retail mutates the live regions before PresentFrame draws them. Final composition
    /// below observes these writes along with later input/network notice callbacks.
    fn draw_world_target(&mut self) {
        use dereth_ui_screens::hud::target::{self, VividTargetIndicator};
        use dereth_ui_screens::screens::gameplay_host::GameCall;
        let Some(shell) = self.shell.ui.as_mut() else {
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
        let world = &self.core.objects.world;
        let mut state = VividTargetIndicator {
            enabled: self.core.teleport.anim.vivid_target_indicator,
            display_on: crate::hud::character_option(
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
        let viewport = self.core.present.size();
        let projection = state.target.and_then(|id| {
            self.core
                .present
                .target_projection(id, self.core.world.as_ref())
        });
        let color = dereth_ui_screens::mapradar::radar::get_blip_color(
            self.core
                .hud
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
        if let Some(shell) = self.shell.ui.as_mut() {
            self.shell.ui_draw_list = shell.draw_list();
            self.core
                .present
                .prepare_ui(&self.core.store, &self.shell.ui_draw_list);
        }
    }

    /// The backpack panel's paper doll.
    ///
    /// The third consumer of the creature-mode preview space, and the only one whose object is
    /// **dressed**. The preview initialization and later redressing behavior,
    /// in full:
    ///
    /// * the viewport is `0x100001D5`; camera `(0.12, -2.4, 0.88)` looking at the origin, one
    ///   `DISTANT_LIGHT` at **2.0** along `(0.3, +1.9, 0.65)`, and sharp mode;
    /// * the object is built from the player -- a **clone of the player's own
    ///   physics object**, so its setup record is the player's, not a heritage default;
    /// * `set_heading(191.3679, 1)` and `set_sequence_animation(<DID for enum 0x10000005>, clear =
    ///   1, low = 1, **0.0** fps)` -- a held pose, not a loop;
    /// * apply the player's visual descriptor to the clone, which is what
    ///   puts the player's *worn gear* on the doll. The player-visual descriptor writer receives
    ///   `0xF625 Item_ObjDescEvent` for the player, which triggers the update.
    ///   On this side it is `ObjectStream::presence(player).objdesc` -- the
    ///   same value `WorldScene::apply_player_objdesc` already dresses the walking body with, and
    ///   **not** the char-gen `ObjDesc`, which is assembled from `CharGenState` and a
    ///   `ClothingTable` and describes a character who does not exist yet.
    /// * receipt of the player description then reads `PropertyInt 0xBC HeritageGroup`
    ///   and applies the heritage-specific camera adjustment for six heritages
    ///   and falls through for the rest.
    ///
    /// The visibility gate is not decoration: `Renderer::draw_ui`'s fallback arm draws a queued
    /// space whose element emitted no blit command, which is right for the portal space (a
    /// transparent full-screen region) and would put the doll over the world whenever the backpack
    /// panel was *closed*. So the queue only happens for an element that is visible with every
    /// ancestor visible -- the same first test used by the region draw path.
    fn paper_doll_use_time(&mut self) {
        use dereth_ui_screens::panels::inventory::paper_doll as pd;
        use dereth_ui_screens::screens::gameplay_host::GameCall;

        let now = self.core.timer.cur_time;
        let dt = (now - self.shell.paper_doll_last_time).clamp(0.0, 0.25);
        self.shell.paper_doll_last_time = now;

        // Where it draws, and whether it draws at all.
        let where_ = self.shell.ui.as_mut().and_then(|shell| {
            let screen = crate::hud_drive::game_screen(&mut shell.flow)?;
            let GameCall::PaperDollViewport(h) = crate::hud_drive::game_call(
                &mut shell.ui,
                screen,
                GameCall::PaperDollViewport(None),
            ) else {
                return None;
            };
            let h = h?;
            if !element_is_drawn(&shell.ui, h) {
                return None;
            }
            let b = shell.ui.screen_clip_box(h);
            b.is_valid().then_some((h, b))
        });
        let Some((who, area)) = where_ else { return };

        // The clone's setup record and the descriptor that dresses it. Both are the *player's*, read
        // off the object stream by player id on this side of the seam.
        let player = self.core.objects.player().or(self.core.hud.player);
        let dress = player
            .and_then(|p| self.core.objects.presence(p))
            .and_then(|p| Some((p.setup_id?, crate::world::to_anim_objdesc(&p.objdesc))));
        let Some((setup, objdesc)) = dress else {
            return;
        };

        let assets = std::sync::Arc::clone(&self.core.anim_assets);
        let store = std::sync::Arc::clone(&self.core.store);
        let id = crate::gpu::PreviewId::PaperDoll;
        let fresh = self.core.present.preview_ensure(id, &assets);
        if fresh {
            self.core.present.preview_use_sharp_mode(id);
            self.core.present.preview_set_light(
                id,
                dereth_world_render::lighting::LightType::Directional,
                pd::LIGHT_INTENSITY,
                dereth_primitives::Vec3::new(
                    pd::LIGHT_DIRECTION.0,
                    pd::LIGHT_DIRECTION.1,
                    pd::LIGHT_DIRECTION.2,
                ),
            );
        }

        // Rebuild when no preview object exists or its visual descriptor changed, because the
        // meshes are baked from the dressed part array. See [`Self::paper_doll_built`].
        if self.shell.paper_doll_built.as_ref() != Some(&(setup, objdesc.clone())) {
            self.core.present.preview_remove_all_objects(id);
            match self
                .core
                .present
                .preview_add_object_dressed(id, &store, setup, Some(&objdesc))
            {
                Ok(Some(_)) => {
                    let anim = crate::assets::enum_did(
                        &*store,
                        crate::preview::UIASSET_GROUP,
                        self.paper_doll_animation_enum(),
                    );
                    self.core
                        .present
                        .preview_set_heading(id, 0, pd::HEADING_DEGREES);
                    match anim {
                        Some(a) => {
                            if !self.core.present.preview_set_sequence_animation(
                                id,
                                0,
                                a,
                                true,
                                pd::LOW_FRAME,
                                pd::FRAMERATE,
                            ) {
                                tracing::warn!("the paper-doll animation {a:?} is not in the dat");
                            }
                        }
                        None => {
                            tracing::warn!("UIASSET PaperDollAnimation does not resolve")
                        }
                    }
                    self.shell.paper_doll_built = Some((setup, objdesc));
                }
                Ok(None) => {
                    tracing::warn!("the paper-doll setup {setup:?} would not load");
                    self.shell.paper_doll_built = None;
                }
                Err(e) => {
                    tracing::warn!("the paper-doll space failed: {e}");
                    self.shell.paper_doll_built = None;
                }
            }
        }

        // Re-apply the heritage camera every tick because setting the camera is idempotent and
        // heritage arrives with `0x0013`, which may be after the first build.
        let heritage = self.paper_doll_heritage();
        let camera = pd::for_race(heritage).map_or(pd::CAMERA_POSITION, |(c, _)| c);
        self.core.present.preview_set_camera_position(
            id,
            dereth_primitives::Vec3::new(camera.0, camera.1, camera.2),
        );
        self.core.present.preview_set_camera_direction(
            id,
            dereth_primitives::Vec3::new(
                pd::CAMERA_TARGET.0,
                pd::CAMERA_TARGET.1,
                pd::CAMERA_TARGET.2,
            ),
        );
        // Zero framerate, so this advances nothing -- it is here because the preview update is
        // what fires the animation's hooks, and because giving the doll a moving pose later must
        // not also require adding the tick.
        self.core.present.preview_use_time(id, dt);

        // The paper-doll selection notice calls
        // the part-selection-lighting start for the item when an item is selected. On that edge
        // (see [`Self::paper_doll_selection_seen`]) it then listens to global message 3, which
        // drives the part-selection-lighting update, the per-frame tick, on the doll's own object.
        let selected = self.core.objects.world.selected;
        if self.shell.paper_doll_selection_seen != selected {
            self.shell.paper_doll_selection_seen = selected;
            if let Some(item) = selected {
                let world = &self.core.objects.world;
                let inventory = player.and_then(|p| world.tables.inventories.get(p));
                let upper = |loc: u32| inventory.and_then(|inv| inv.upper_inv_obj(loc));
                let mask = crate::preview::PaperDollSelectionLighting::selection_mask_from_object(
                    item,
                    world.player,
                    &upper,
                );
                let doll = self.core.present.preview_part_array_mut(id, 0);
                self.shell.paper_doll_lighting.begin(mask, now, doll);
            }
        }
        let doll = self.core.present.preview_part_array_mut(id, 0);
        self.shell.paper_doll_lighting.update(now, doll);

        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: proves x1 >= x0 and y1 >= y0; `max(0)` covers an element laid
        // out off the top-left edge. The box is inclusive, hence the +1.
        let rect = dereth_render::camera::Viewport {
            x: area.x0.max(0) as u32,
            y: area.y0.max(0) as u32,
            width: (area.x1 - area.x0.max(0) + 1).max(0) as u32,
            height: (area.y1 - area.y0.max(0) + 1).max(0) as u32,
        };
        self.core.present.preview_queue(id, who, rect);
    }

    /// **The identify window's 3D portrait.**
    ///
    /// Appraising a creature or an NPC shows the creature beside the numbers. The viewport element
    /// is registered as engine class `0x0D` (`factory.rs`), and [`crate::preview::PreviewSpace`]
    /// drives it as it drives the three other viewports; this is what binds `0x10000148` and
    /// queues a space for it. Without it the element is built from the layout and draws an
    /// empty box.
    ///
    /// # What the client does, in its own order
    ///
    /// Appraisal delivery picks the pane and initializes it for the object before updating its
    /// text. Initialization is virtual: the item-examine panel inherits the two-store base behavior, which is
    /// why an appraised **item** gets no portrait. The basic-creature examine panel overrides it with:
    ///
    /// ```text
    /// set a distant light with intensity 2.0 and direction (0.3, 1.9, 0.65)
    /// remove all preview objects
    /// discard any previous preview object
    /// p = look up the live physics object by object id
    /// if p exists:
    ///     clone p for the preview
    ///     set the clone's heading to 191.3679 degrees
    ///     compute its bounding box
    ///     set the camera from `portrait::camera_position`
    ///     add the clone to the preview
    /// end if
    /// ```
    ///
    /// # Where this deviates, and why it is the same behaviour
    ///
    /// * **Rebuild trigger.** Panel initialization runs once per `0x00C9` reply and never per frame; this runs
    ///   per frame and rebuilds when `Self::examine_3d_built` disagrees with the panel's current
    ///   object. Same builds, driven from the state rather than from the edge, which is what the
    ///   other three viewports here already do.
    /// * **Clone order.** Retail boxes the clone *before* adding it; the box is taken here after
    ///   the add, because [`crate::preview::PreviewSpace::add_object_dressed`] is what establishes
    ///   the part frames at all. Adding only sets cell membership and
    ///   placement frame 0, both of which the add already applied, so the box is of the
    ///   same posed object either way.
    /// * **Dressing.** Object creation clones a live physics object, which is already wearing its
    ///   `ObjDesc`; the clone here is rebuilt from the setup, so the descriptor is applied
    ///   explicitly. Without it an appraised player would be portrayed naked.
    ///
    /// **No animation is set**, deliberately: initialization sets no sequence animation, so the
    /// object keeps its setup-default animation, which
    /// `add_object_dressed` already applies. The portrait is therefore *live* — retail's
    /// update path advances every visible preview object each frame, and
    /// [`crate::preview::PreviewSpace::use_time`] provides that behavior here.
    fn examine_3d_use_time(&mut self) {
        use dereth_ui_screens::panels::examination::portrait;
        use dereth_ui_screens::screens::gameplay_host::GameCall;

        let now = self.core.timer.cur_time;
        let dt = (now - self.shell.examine_3d_last_time).clamp(0.0, 0.25);
        self.shell.examine_3d_last_time = now;

        // Where it draws, whether it draws at all, and which object the panel is showing.
        //
        // The `ExamineSubUi::Item` arm is the virtual `Init` fork: the item pane's `Init` adds no
        // object, so an appraised item must leave the space untouched rather than portray itself.
        let where_ = self.shell.ui.as_mut().and_then(|shell| {
            let screen = crate::hud_drive::game_screen(&mut shell.flow)?;
            let GameCall::ExaminePreview(p) =
                crate::hud_drive::game_call(&mut shell.ui, screen, GameCall::ExaminePreview(None))
            else {
                return None;
            };
            let (item_pane, current, viewport) = p?;
            if item_pane {
                return None;
            }
            let object = current?;
            let h = viewport?;
            if !element_is_drawn(&shell.ui, h) {
                return None;
            }
            let b = shell.ui.screen_clip_box(h);
            b.is_valid().then_some((h, b, object))
        });
        let Some((who, area, object)) = where_ else {
            return;
        };

        // Look up the *live* object by id. A null there
        // is the client's `if (p)` and leaves the space with whatever it last held.
        let dress = self
            .core
            .objects
            .presence(object)
            .and_then(|p| Some((p.setup_id?, crate::world::to_anim_objdesc(&p.objdesc))));
        let Some((setup, objdesc)) = dress else {
            return;
        };

        let assets = std::sync::Arc::clone(&self.core.anim_assets);
        let store = std::sync::Arc::clone(&self.core.store);
        let id = crate::gpu::PreviewId::Examine;
        let fresh = self.core.present.preview_ensure(id, &assets);
        let rebuild = fresh || self.shell.examine_3d_built != Some((object, setup));

        if rebuild {
            // Setting this light is the first initialization act and removes all lights, adds one,
            // and sets its direction underneath, so
            // re-issuing it on every rebuild is what the client does rather than an extra.
            self.core.present.preview_set_light(
                id,
                dereth_world_render::lighting::LightType::Directional,
                portrait::LIGHT_INTENSITY,
                dereth_primitives::Vec3::new(
                    portrait::LIGHT_DIRECTION.0,
                    portrait::LIGHT_DIRECTION.1,
                    portrait::LIGHT_DIRECTION.2,
                ),
            );
            self.core.present.preview_remove_all_objects(id);
            match self
                .core
                .present
                .preview_add_object_dressed(id, &store, setup, Some(&objdesc))
            {
                Ok(Some(_)) => {
                    self.core
                        .present
                        .preview_set_heading(id, 0, portrait::HEADING_DEGREES);
                    self.shell.examine_3d_built = Some((object, setup));
                }
                Ok(None) => {
                    tracing::warn!("the identify portrait's setup {setup:?} would not load");
                    self.shell.examine_3d_built = None;
                }
                Err(e) => {
                    tracing::warn!("the identify portrait's space failed: {e}");
                    self.shell.examine_3d_built = None;
                }
            }
        }

        // Read the bounding box, then set the camera, re-issued every tick: the element's box is the other
        // half of the arithmetic and a resized or re-laid-out panel must re-frame the creature.
        // Setting the camera is two stores and a rotate, so this is cheap and idempotent.
        let bb = self.core.present.preview_object_bounding_box(id, 0, &store);
        if let Some(bb) = bb {
            let pos = portrait::camera_position(bb.min, bb.max, area.width(), area.height());
            self.core.present.preview_set_camera_position(id, pos);
            self.core.present.preview_set_camera_direction(
                id,
                dereth_primitives::Vec3::new(
                    portrait::CAMERA_DIRECTION.0,
                    portrait::CAMERA_DIRECTION.1,
                    portrait::CAMERA_DIRECTION.2,
                ),
            );
            // The portrait renderer's `update_position` loop makes the
            // portrait live rather than a still.
            self.core.present.preview_use_time(id, dt);
        }

        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: proves x1 >= x0 and y1 >= y0; `max(0)` covers an element laid
        // out off the top-left edge. The box is inclusive, hence the +1.
        let rect = dereth_render::camera::Viewport {
            x: area.x0.max(0) as u32,
            y: area.y0.max(0) as u32,
            width: (area.x1 - area.x0.max(0) + 1).max(0) as u32,
            height: (area.y1 - area.y0.max(0) + 1).max(0) as u32,
        };
        self.core.present.preview_queue(id, who, rect);
    }

    /// `PropertyInt 0xBC HeritageGroup` off the `0x0013` qualities, which is what
    /// the player-description receiver hands to the heritage-specific camera adjustment.
    ///
    /// Zero is the integer-property query's own answer for a property the description did not carry, and zero is
    /// also the heritage table's fall-through — so an absent heritage keeps the initial camera, which
    /// is exactly what the client does.
    fn paper_doll_heritage(&self) -> u32 {
        let h = self
            .core
            .hud
            .player_desc(&self.core.objects.world)
            .map_or(0, |q| {
                q.inq_int(dereth_ui_screens::panels::inventory::HERITAGE_GROUP_PROPERTY)
            });
        u32::try_from(h).unwrap_or(0)
    }

    /// `0x10000005 PaperDollAnimation`, replaced by the two
    /// Olthoi heritages' own enums during heritage-specific preview adjustment.
    fn paper_doll_animation_enum(&self) -> u32 {
        use dereth_ui_screens::panels::inventory::{paper_doll as pd, PAPER_DOLL_ANIMATION_ENUM};
        pd::for_race(self.paper_doll_heritage())
            .and_then(|(_, a)| a)
            .unwrap_or(PAPER_DOLL_ANIMATION_ENUM)
    }

    /// The 3D character preview's slot: keep the char-gen preview space in step with the wizard
    /// and say where it draws this frame.
    ///
    /// This is the host half of the preview update and camera setup. The
    /// *model* is [`dereth_ui_screens::screens::chargen::Cg3dView`], which the wizard fills in: the setup, the heading, the camera and the animation **enums**. Nothing
    /// here decides anything about the wizard; it resolves a dat id, moves a camera and points at
    /// a rectangle.
    ///
    /// Three pieces of the client's own bookkeeping are reproduced rather than simplified:
    ///
    /// * the object is rebuilt only when the setup id changes, which is the update's own test -- a
    ///   rebuild every frame would re-upload the body's textures sixty times a second;
    /// * the animation start and stop are re-issued on the
    ///   *animating* edge, at **30 fps** and **0 fps** respectively, with `clear = 1` both times;
    /// * the light and sharp mode are the space's, not the object's, and
    ///   the light direction is `(0.3, **1.9**, 0.65)` -- positive y, where the portal space's is
    ///   negative.
    fn preview_use_time(&mut self) {
        use dereth_ui_screens::screens::pregame_host::PregameCall;

        let now = self.core.timer.cur_time;
        // Elapsed seconds since the last tick, clamped exactly as the world's own delta is.
        // Nothing below accumulates a per-frame increment: takes `dt`.
        let dt = (now - self.shell.preview_last_time).clamp(0.0, 0.25);
        self.shell.preview_last_time = now;

        // The appearance page's global-message 3 arm is the
        // per-frame tick: zoom animation when a zoom is animating, then rotation when rotating.
        // `CharGenScreen::tick_preview` is the rotation step: the two rotate arrows set the
        // rotating flag and direction, and this is what advances the heading. Driven off elapsed
        // seconds, like everything else in this step.
        if let Some(shell) = self.shell.ui.as_mut() {
            let wizard_took = shell.flow.current_mut().is_some_and(|w| {
                crate::hud_drive::pregame_call(
                    &mut shell.ui,
                    &mut **w,
                    PregameCall::TickPreview(dt),
                )
                .0
            });
            if let (false, Some(w)) = (wizard_took, crate::hud_drive::game_screen(&mut shell.flow))
            {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    w,
                    dereth_ui_screens::screens::gameplay_host::GameCall::BarberTick(dt),
                );
            }
        }

        // What the wizard wants drawn, and where. `screen_clip_box` rather than `screen_box`: a
        // viewport scrolled under its parent must draw inside the parent's clipping rectangle.
        let want = self.shell.ui.as_mut().and_then(|shell| {
            let wizard = {
                let w = shell.flow.current_mut()?;
                match crate::hud_drive::pregame_call(
                    &mut shell.ui,
                    &mut **w,
                    PregameCall::WizardPreview(None),
                ) {
                    (true, PregameCall::WizardPreview(p)) => p,
                    _ => None,
                }
            };
            let (view3d, state, tables) = match wizard {
                Some(w) => w,
                None => {
                    use dereth_ui_screens::screens::gameplay_host::GameCall;
                    let screen = crate::hud_drive::game_screen(&mut shell.flow)?;
                    let GameCall::BarberPreview(barber) = crate::hud_drive::game_call(
                        &mut shell.ui,
                        screen,
                        GameCall::BarberPreview(None),
                    ) else {
                        return None;
                    };
                    barber?
                }
            };
            let screen = shell.flow.current()?;
            let id = view3d.viewport?;
            let root = *screen.roots().first()?;
            let h = shell.ui.get_child_recursive(root, id)?;
            let b = shell.ui.screen_clip_box(h);
            // Character-generation preview dressing needs the state and tables as well as the
            // view, because the `ObjDesc` that dresses the default parts is built from them. Both are cheap to carry -- `CharGenState` is plain data and the tables are an
            // `Rc` -- and taking them here keeps the whole read of the shell in one borrow.
            b.is_valid().then_some((view3d, h, b, state, tables))
        });
        let Some((view3d, who, area, cg_state, cg_tables)) = want else {
            return;
        };

        let assets = std::sync::Arc::clone(&self.core.anim_assets);
        let id = crate::gpu::PreviewId::CharGen;
        self.core.present.preview_ensure(id, &assets);
        let store = std::sync::Arc::clone(&self.core.store);

        // Assemble the character-generation preview's appearance — everything that dresses the model.
        // Without it the turntable draws the naked setup record: the arrows move `CharGenState`'s
        // indices and the wizard rebuilds the view, but the model is never dressed. See
        // [`crate::preview::chargen_objdesc`].
        let (objdesc, dress_stats) = match cg_tables.as_ref() {
            Some(t) => {
                let s = std::sync::Arc::clone(&self.core.store);
                let cache = &mut self.shell.chargen_pal_sets;
                crate::preview::chargen_objdesc(
                    &t.chargen,
                    &cg_state,
                    &t.clothing,
                    view3d.setup,
                    &mut |id| cache.palettes(&s, id),
                )
            }
            None => (
                dereth_animation::parts::ObjDesc::default(),
                crate::preview::ChargenDressStats::default(),
            ),
        };
        self.shell.chargen_dress = dress_stats;

        // The preview object's setup-changed test, the background object's own
        // environment-setup-changed test, and the animation edge, together.
        //
        // The `ObjDesc` is part of the key for the reason the paper doll's is: the part
        // meshes are baked from the **dressed** part array
        // ([`crate::preview::PreviewSpace::add_object_dressed`]), so a redress is a rebuild. The
        // client can be cheaper -- it reapplies the descriptor to the object it
        // already has -- and gets the same picture.
        //
        // The two animation enums are in the key because the block below reads them. See
        // [`ChargenPreviewKey`].
        let key = ChargenPreviewKey::from_view(&view3d, objdesc.clone());
        if self.shell.preview_chargen.as_ref() != Some(&key) {
            let rebuild =
                !matches!(self.shell.preview_chargen.as_ref(), Some(k) if k.same_space(&key));
            if rebuild {
                self.core.present.preview_remove_all_objects(id);
                // Enable sharp rendering after rebuilding the preview; this is the preview update's
                // final operation and the reason the turntable is sharper than the world.
                self.core.present.preview_use_sharp_mode(id);
                self.core.present.preview_set_light(
                    id,
                    dereth_world_render::lighting::LightType::Directional,
                    2.0,
                    dereth_primitives::Vec3::new(0.3, 1.9, 0.65),
                );
                // The player object stays **index 0**, which is what `set_heading` and
                // `set_sequence_animation` below address; the client holds a pointer
                // to the player object and adds the background first, so its indices are the other
                // way round. Nothing observable turns on the order: the pass clears depth and
                // draws opaque before blended for the whole space, so the two objects resolve
                // against the Z-buffer either way.
                match self.core.present.preview_add_object_dressed(
                    id,
                    &store,
                    key.setup,
                    Some(&objdesc),
                ) {
                    Ok(Some(_)) => {}
                    Ok(None) => tracing::warn!(
                        "the char-gen preview setup {:?} would not load",
                        view3d.setup
                    ),
                    Err(e) => tracing::warn!("the char-gen preview failed: {e}"),
                }
                // The background object -- the room the model stands in, the heritage group's
                // environment setup. `INVALID_DID` means the heritage has none and the client
                // adds nothing.
                if view3d.bg_setup.0 != 0 {
                    match self
                        .core
                        .present
                        .preview_add_object(id, &store, view3d.bg_setup)
                    {
                        Ok(Some(_)) => {}
                        Ok(None) => tracing::warn!(
                            "the char-gen background {:?} would not load",
                            view3d.bg_setup
                        ),
                        Err(e) => tracing::warn!("the char-gen background failed: {e}"),
                    }
                }
            }
            // Start / stop the animation.
            let (enum_value, framerate) = if view3d.animating {
                (view3d.animation_enum, 30.0)
            } else {
                (view3d.rest_animation_enum, 0.0)
            };
            match crate::assets::enum_did(&*store, crate::preview::UIASSET_GROUP, enum_value) {
                Some(a) => {
                    self.core.present.preview_clear_sequence_anims(id, 0);
                    if !self
                        .core
                        .present
                        .preview_set_sequence_animation(id, 0, a, true, 0, framerate)
                    {
                        tracing::warn!("the char-gen animation {a:?} is not in the dat");
                    }
                }
                None => {
                    tracing::warn!("UIASSET enum {enum_value:#010X} does not resolve");
                }
            }
            self.shell.preview_chargen = Some(key);
        }

        let p = view3d.camera_position;
        let d = view3d.camera_direction;
        self.core
            .present
            .preview_set_camera_position(id, dereth_primitives::Vec3::new(p[0], p[1], p[2]));
        self.core
            .present
            .preview_set_camera_direction(id, dereth_primitives::Vec3::new(d[0], d[1], d[2]));
        self.core.present.preview_set_heading(id, 0, view3d.heading);
        self.core.present.preview_use_time(id, dt);
        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: above proves x1 >= x0 and y1 >= y0, and the `max(0)` covers
        // an element laid out off the left or top edge. The box is inclusive, hence the +1.
        let rect = dereth_render::camera::Viewport {
            x: area.x0.max(0) as u32,
            y: area.y0.max(0) as u32,
            width: (area.x1 - area.x0.max(0) + 1).max(0) as u32,
            height: (area.y1 - area.y0.max(0) + 1).max(0) as u32,
        };
        self.core.present.preview_queue(id, who, rect);
    }

    /// The teleport tunnel's swirl.
    ///
    /// The tunnel is not just a hidden world: it is a preview space, built and driven from its UI
    /// update:
    ///
    /// * one object, the enum `0x10000001` setup -- `portalspace_background`;
    /// * one `DISTANT_LIGHT` at intensity **2.0**, direction `(0.3, -1.9, 0.65)` -- **negative**
    ///   y, where the 3D character preview's is positive;
    /// * the camera at `(0.24, -2.7, 0.88)`, re-issued every tunnel frame by the update;
    /// * the smart-box field of view, which makes the teleport's projection collapse apply to the
    ///   swirl as well as the world;
    /// * `set_sequence_animation(<DID for enum 0x10000002>, clear = 1, low = 1, **40.0** fps)`,
    ///   started when the portal-space element becomes visible, and cleared when the
    ///   tunnel ends;
    /// * a camera direction of `(0, current rotation angle, 0)` every frame, which is the eased
    ///   spin the teleport animation computes and exposes as `TeleportAnim::rotation_angle`.
    ///
    /// A 1.1 scale is **not** applied to the portal space: the client's 1.1 scale is set on the
    /// *world* camera, unconditionally.
    fn portal_space_use_time(&mut self) {
        let now = self.core.timer.cur_time;
        // Elapsed seconds, on the same terms as the char-gen space's: nothing accumulated.
        let dt = (now - self.shell.portal_last_time).clamp(0.0, 0.25);
        self.shell.portal_last_time = now;
        let id = crate::gpu::PreviewId::Portal;
        let tunnel = self.core.teleport.anim.state.is_tunnel();
        if !tunnel {
            // The tunnel fade-out's end: clear the teleport object's sequence anims and hide.
            if self.core.present.preview_has_anims(id, 0) {
                self.core.present.preview_clear_sequence_anims(id, 0);
            }
            self.core.teleport.portal_anim_frame = None;
            return;
        }

        let assets = std::sync::Arc::clone(&self.core.anim_assets);
        let store = std::sync::Arc::clone(&self.core.store);
        if self.core.present.preview_ensure(id, &assets) {
            // Start the portal-space animation once, on the transition to visible.
            let obj = crate::assets::enum_did(
                &*store,
                crate::preview::UIASSET_GROUP,
                crate::preview::ENUM_PORTALSPACE_BACKGROUND,
            );
            match obj {
                Some(o) => match self.core.present.preview_add_object(id, &store, o) {
                    Ok(Some(_)) => {}
                    Ok(None) => tracing::warn!("the portal object {o:?} would not load"),
                    Err(e) => tracing::warn!("the portal space failed: {e}"),
                },
                None => tracing::warn!("UIASSET portalspace_background does not resolve"),
            }
            {
                use dereth_ui_screens::screens::teleport::portal_space as ps;
                self.core.present.preview_set_light(
                    id,
                    dereth_world_render::lighting::LightType::Directional,
                    ps::LIGHT_INTENSITY,
                    dereth_primitives::Vec3::new(
                        ps::LIGHT_DIRECTION.0,
                        ps::LIGHT_DIRECTION.1,
                        ps::LIGHT_DIRECTION.2,
                    ),
                );
                self.core.present.preview_use_world_fov(id);
            }
        }

        // The update's portal-space-not-visible arm: start the sequence, place the camera,
        // show the space and hide the world. The teleport animation has already hidden the world.
        let anim = crate::assets::enum_did(
            &*store,
            crate::preview::UIASSET_GROUP,
            crate::preview::ENUM_PORTALSPACE_ANIMATION,
        );
        let angle = self.core.teleport.anim.rotation_angle;
        {
            use dereth_ui_screens::screens::teleport::{portal_space as ps, timing};
            if !self.core.present.preview_has_anims(id, 0) {
                if let Some(a) = anim {
                    #[allow(clippy::cast_possible_truncation)]
                    // LINT-OK: `set_sequence_animation`'s framerate argument is a `float` literal
                    // in the client -- 40.0.
                    let fps = timing::PORTAL_FRAMERATE as f32;
                    if !self
                        .core
                        .present
                        .preview_set_sequence_animation(id, 0, a, true, 1, fps)
                    {
                        tracing::warn!("portalspace_animation {a:?} is not in the dat");
                    }
                }
            }
            self.core.present.preview_set_camera_position(
                id,
                dereth_primitives::Vec3::new(
                    ps::CAMERA_POSITION.0,
                    ps::CAMERA_POSITION.1,
                    ps::CAMERA_POSITION.2,
                ),
            );
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: the client's current rotation angle is a `double`, while the preview camera direction
            // is a `Vec3` of `float`s; the client narrows it here too.
            self.core.present.preview_set_camera_direction_degrees(
                id,
                dereth_primitives::Vec3::new(0.0, angle as f32, 0.0),
            );
            self.core.present.preview_use_time(id, dt);
        }
        // The sequence's real frame counter.
        self.core.teleport.portal_anim_frame = self.core.present.preview_curr_frame_number(id, 0);

        // Where it draws: the whole world-controller UI viewport region. The portal-space element is
        // element `0x10000436`, an 800x600 viewport element under `0x10000037` in layout
        // `0x2100000F`.
        let who = self.shell.ui.as_ref().and_then(|shell| {
            let root = *shell.flow.current()?.roots().first()?;
            let h = shell
                .ui
                .get_child_recursive(root, dereth_ui_screens::hud::world_view::PORTAL_SPACE)?;
            let b = shell.ui.screen_box(h);
            b.is_valid().then_some((h, b))
        });
        let (w, h) = self.core.present.size();
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
                self.core.present.preview_queue(id, handle, rect);
            }
            // No shell at all (a `--no-ui` run), or a shell whose tree has no smart box: the
            // space is the whole window, which is what the element's own box is anyway. The
            // handle names no live element, so `draw_ui`'s fallback pass is what runs it.
            None => self.core.present.preview_queue(
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

    /// Update cursor state from this build's five live inputs.
    ///
    /// Gathers the five inputs, runs [`crate::cursor::update_cursor_state`], and drains whatever
    /// the UI cursor setter lets through onto the window. Where each input comes from,
    /// and which of them have a writer today:
    ///
    /// | input | source | has a writer? |
    /// |---|---|---|
    /// | busy count | the world's busy count (`dereth_client_model::magic::MagicState::busy_count`) | yes |
    /// | target mode | [`crate::interaction::Interaction::target_mode`] | yes, all four modes |
    /// | combat mode | `dereth_client_model::.combat_mode` | yes |
    /// | found object id | [`crate::pick::WorldPicker::click_object`] | live mouse move / global loop: synchronous exact item-slot identity, or completed world-draw geometry pick |
    /// | target compatibility | [`crate::cursor::is_target_compatible_with_targeting_object`] | yes, |
    ///
    /// **The busy count** is one shared counter. A teleport (and the log-in's portal space, and a
    /// log-off's fade) raises it until the world fades back in; a cast, a use, a targeted use and
    /// a shop request raise it until the use-done acknowledgement; a swing the server commenced
    /// raises it until the attack is done; an examine raises it until its answer; and the
    /// allegiance panel's request raises it until the allegiance update answers it.
    fn update_cursor_state(&mut self) {
        let found = self.core.interaction.pick.click_object().0;
        let inputs = crate::cursor::CursorInputs {
            // The hourglass is up while anything the player asked for is still waiting.
            busy: self.core.objects.world.magic.busy_count,
            target_mode: self.core.interaction.target_mode().into(),
            combat_mode: self.core.objects.world.combat.combat_mode,
            hovering: found.0 != 0,
            // Ask whether the found object is compatible with the targeting object.
            // The original item survives selection changes until target acquisition consumes it.
            target_compatible: crate::cursor::is_target_compatible_with_targeting_object(
                &self.core.objects.world,
                self.core.objects.world.targeting_object,
                found,
            ),
        };
        // `None` on a `--no-ui` run, which gates the
        // `SetCursor` and nothing else.
        let mut shell = self.shell.ui.as_mut();
        let ui = shell.as_mut().map(|s| &mut s.ui);
        self.shell
            .cursor
            .update_cursor_state(&*self.core.store, ui, inputs);
        if let Some(s) = self.shell.ui.as_mut() {
            self.shell.cursor.apply_pending(&self.core.store, &mut s.ui);
        }
    }
}

impl<H: Host> Shell for ClientShell<H> {
    type Panels = dereth_ui_screens::panels::remaining::RemainingPanels;
    type Hud = crate::hud::Hud;
    type Present = dyn ClientPresentation;

    /// The keymap first: the user's `.keymap` is merged **first** so a rebound key wins over both
    /// shipped defaults, and the merged map is written back on exit. Not fatal: a client with no
    /// key bindings is useless but can still draw, and saying so is better than refusing to start.
    fn start_input(&mut self, app: &mut CoreApp<H>) {
        let preferences_file = app.config().preferences_file.clone();
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
        match crate::input::InputShell::new(&*app.store, keymap.as_ref()) {
            Ok(i) => self.input = Some(i),
            Err(e) => tracing::warn!("no input: {e}"),
        }
    }

    /// The window's queued events, in arrival order, then the end of the drain.
    fn window_input(&mut self, app: &mut CoreApp<H>, time_ms: u32) {
        let events: Vec<_> = self.window_events.borrow_mut().drain(..).collect();
        for event in &events {
            route_host_event(app, self, event, time_ms);
        }
    }

    fn input_use_time(&mut self, _app: &mut CoreApp<H>, now: dereth_primitives::LocalTime) {
        if let Some(input) = self.input.as_mut() {
            input.use_time(now);
        }
    }

    fn hand_on_actions(&mut self, actions: &mut dereth_client_runtime::actions::ActionQueue) {
        if let Some(input) = self.input.as_mut() {
            input.hand_on(actions);
        }
    }

    fn control_notice(&mut self, notice: dereth_client_runtime::shell::ControlNotice) {
        if let Some(input) = self.input.as_mut() {
            input.apply_notice(notice);
        }
    }

    /// The key map, when it was loaded and its configured filename is non-empty: keymap
    /// serialization writes the **full merged map**, defaults included, and the next run merges it
    /// back first so it wins. `Nothing` is the client's own skip when that filename is empty.
    fn save_bindings(&mut self) -> dereth_client_runtime::shutdown::Outcome {
        use dereth_client_runtime::shutdown::Outcome;
        match self
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

    fn start_ui(&mut self, app: &mut CoreApp<H>) -> Result<(), StartupError> {
        Ui {
            core: app,
            shell: self,
        }
        .start_ui()
    }

    fn has_ui(&self) -> bool {
        self.ui.is_some()
    }

    /// Whether the current UI mode is the gameplay screen, which means "the player is in the world".
    fn in_gameplay(&self) -> bool {
        self.ui
            .as_ref()
            .is_some_and(|s| s.flow.current_mode() == Some(dereth_ui::framework::mode::GAME_PLAY))
    }

    /// Credits has no backdrop element. Retail's black is the frame-start clear:
    /// the character-management screen builds the rotating preview, while the credits screen
    /// only creates its two authored roots, so entering Credits hides the retained world.
    fn hides_world(&self) -> bool {
        self.ui.as_ref().and_then(|shell| shell.flow.current_mode())
            == Some(dereth_ui::framework::mode::CREDITS)
    }

    /// A Turbine callback is a synchronous notice to the CURRENT chat subscribers. Preserve that
    /// generation through the deferred Hud delivery, not across a rebuild.
    fn chat_generation(&self) -> Option<u64> {
        self.ui
            .as_ref()
            .and_then(|shell| shell.flow.current())
            .and_then(|s| s.is_game().then_some(self.gameplay_serial))
    }

    fn ui_requests(&mut self) -> Option<&mut dereth_client_contract::requests::Outbox> {
        self.ui.as_mut().map(|s| &mut s.ui.requests)
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

    /// Is the pointer over the 3-D view, rather than over a HUD window drawn on
    /// top of it? Retail asks with a rectangle that degenerates to the whole window in the
    /// shipped layout, so this build asks the hit test instead -- the same machinery the click
    /// path consults. `true` with no UI at all.
    fn pointer_over_game_view(&self, cursor: (i32, i32)) -> bool {
        let Some(shell) = self.ui.as_ref() else {
            return true;
        };
        let (x, y) = cursor;
        pointer_over_game_view_at(shell, shell.ui.hit_test_screen(x, y))
    }

    /// Look up element `0x100005F7` and read its visibility bit: is `<EXAM>` on
    /// screen? `false` covers both negative legs: no UI manager and no such element.
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

    /// Use the panel's own hide path so the key and close button
    /// reach one statement and one counter.
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

    /// Character-error and server-died notices, the two edges that reach the disconnected screen
    /// from anywhere; and account-booted and account-banned responses, the two that reach it without a notice and carry their own sentence.
    ///
    /// `character_error_string_id` returning `None` is the switch's `default:` arm, which "leaves
    /// the `StringInfo` untouched and **never queues a mode**". The literals the two booted and
    /// banned arms format are [`dereth_ui_screens::screens::disconnected::BOOTED_FORMAT`] and its
    /// neighbours.
    fn disconnect_message(
        &self,
        e: &dereth_client_net::client_session::SessionEvent,
        clock: &dyn ClockSource,
    ) -> Option<String> {
        use dereth_client_net::client_session::{DisconnectReason, SessionEvent, SessionState};
        use dereth_ui_screens::screens::disconnected as screen;
        match e {
            // Send the character-error notice for this code.
            SessionEvent::CharacterError(code) => {
                screen::character_error_string_id(*code).map(str::to_string)
            }
            // The session independently decides both the 110-second world-entry timeout and the
            // 40-second heartbeat gap.
            SessionEvent::StateChanged(SessionState::Disconnected(
                DisconnectReason::ServerDied,
            )) => Some(screen::SERVER_DIED_STRING_ID.to_string()),
            // `None` is a `0xF7DC` with no body
            // at all, which the handler cannot tell from an empty reason; both take the default.
            SessionEvent::AccountBooted(reason) => {
                Some(screen::account_booted_message(reason.as_deref()))
            }
            // The clock is an input because the
            // handler reads real time at the moment the message lands, and the
            // zone is an input because it does `asctime(localtime(&t))`.
            SessionEvent::AccountBanned { expiry, reason } => {
                let now = clock.unix_secs();
                // The zone is read for the **expiry** instant, not for now: a ban that ends after
                // a daylight change is announced in the zone it will end in, which is what
                // `localtime` does with the already-summed `t`.
                let at = screen::ban_expiry_epoch(*expiry, now);
                Some(screen::account_banned_message(
                    *expiry,
                    reason,
                    now,
                    clock.utc_offset_secs(at),
                ))
            }
            _ => None,
        }
    }

    fn service_dialogs(
        &mut self,
        interaction: &mut crate::interaction::Interaction,
        world: &mut dereth_client_model::World,
        now: dereth_primitives::LocalTime,
    ) {
        self.targeted_dialogs
            .service(self.ui.as_mut(), interaction, world, now);
    }

    fn before_ui_input(
        &mut self,
        chat: &mut dereth_client_model::chat::ChatState,
        player_airborne: bool,
        chat_focus_notices: Vec<dereth_client_model::chat::TalkFocusNotice>,
    ) {
        if let Some(shell) = self.ui.as_mut() {
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    screen,
                    dereth_ui_screens::screens::gameplay_host::GameCall::PlayerAirborne(
                        player_airborne,
                    ),
                );
                crate::hud::deliver_chat_focus_notices(
                    &mut shell.ui,
                    screen,
                    chat,
                    chat_focus_notices,
                );
            }
        }
    }

    fn drive_pregame_screens(&mut self, app: &mut CoreApp<H>) {
        Ui {
            core: app,
            shell: self,
        }
        .drive_pregame_screens();
    }

    fn drive_world_script(&mut self, app: &mut CoreApp<H>, now: dereth_primitives::LocalTime) {
        Ui {
            core: app,
            shell: self,
        }
        .drive_world_script(now.0);
        Ui {
            core: app,
            shell: self,
        }
        .drive_say(now.0);
    }

    fn ui_frame(
        &mut self,
        app: &mut CoreApp<H>,
        now: dereth_primitives::LocalTime,
        notices: UiNotices,
    ) {
        Ui {
            core: app,
            shell: self,
        }
        .ui_frame(now, notices);
    }

    fn deliver_chat_focus_notices(
        &mut self,
        chat: &mut dereth_client_model::chat::ChatState,
        notices: Vec<dereth_client_model::chat::TalkFocusNotice>,
    ) {
        if let Some(shell) = self.ui.as_mut() {
            if let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) {
                crate::hud::deliver_chat_focus_notices(&mut shell.ui, screen, chat, notices);
            }
        }
    }

    /// Jump's notices are synchronous with their input/control-loss owner. Finish the
    /// existing subscriber before the next action, not next frame. An absent or outgoing
    /// subscriber leaves no retained notice history.
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
        let writes = u64::from(crate::hud::deliver_power_bar_notices(
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
        inter: &mut crate::interaction::Interaction,
        world: &mut dereth_client_model::World,
        notice: &dereth_client_model::Notice,
    ) {
        dispatch_object_panel_notice(self.ui.as_mut(), hud, inter, world, notice);
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
        layout_commands: Vec<crate::interaction::UiLayoutCommand>,
    ) {
        if let Some(shell) = self.ui.as_mut() {
            for command in layout_commands {
                let result = match command {
                    crate::interaction::UiLayoutCommand::Save(name) => shell
                        .screen_layout_path(&name, prefs, character, world)
                        .map(|path| shell.save_ui_layout(&path)),
                    crate::interaction::UiLayoutCommand::Load(name) => shell
                        .screen_layout_path(&name, prefs, character, world)
                        .map(|path| shell.load_ui_layout(&path)),
                    crate::interaction::UiLayoutCommand::SetLockUi(locked) => {
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
        view: &crate::hud::HudView<'_>,
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

    fn world_tooltip(&mut self, interaction: &mut crate::interaction::Interaction) {
        if let Some(shell) = self.ui.as_mut() {
            apply_world_tooltip(shell, interaction);
        }
    }

    fn draw_world_target(&mut self, app: &mut CoreApp<H>) {
        Ui {
            core: app,
            shell: self,
        }
        .draw_world_target();
    }

    fn update_cursor(&mut self, app: &mut CoreApp<H>) {
        Ui {
            core: app,
            shell: self,
        }
        .update_cursor_state();
    }

    /// Copy the completed frame into the UI image, and the mirror `Paste` reads
    /// back: `dereth-ui` records what only a host with a window can perform. Both directions are
    /// cheap -- the send happens only on a Copy, the read only when the clipboard's sequence number
    /// moved.
    fn sync_clipboard(&mut self) {
        if let Some(s) = self.ui.as_mut() {
            self.clipboard.sync(&mut s.ui, &mut self.host_clipboard);
        }
    }

    fn compose_ui(&mut self, app: &mut CoreApp<H>) {
        Ui {
            core: app,
            shell: self,
        }
        .compose_ui_draw_list();
    }

    /// Ending the frame with `true` is three things in one call: **the 2D UI overlay**,
    /// `EndScene` and `Present`. The overlay is composited over the finished 3D frame with the
    /// depth test off, which is why it is inside this step and not a step of its own.
    fn draw_ui(
        &mut self,
        present: &mut Self::Present,
    ) -> Result<(), dereth_client_runtime::present::PresentError> {
        present.draw_ui(&self.ui_draw_list)
    }

    /// Broadcast the global refresh message — `UiSystem::refresh_event`, which re-lays the root
    /// out at the new extent and pushes `UIGlobalMessage 0x0E` at every registered listener.
    fn set_display(&mut self, display: (i32, i32)) {
        if let Some(shell) = self.ui.as_mut() {
            shell.set_display(display);
        }
    }

    /// UI cleanup: the flow and every root element it holds, then the element manager, with the
    /// texture slots they held handed back first.
    fn cleanup_ui(&mut self, app: &mut CoreApp<H>) {
        let ui = self.ui.take();
        if ui.is_some() {
            let r = app.present.release_ui_textures();
            self.ui_release.freed += r.freed;
            self.ui_release.still_linked += r.still_linked;
            self.ui_release.unknown += r.unknown;
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
    interaction: &mut crate::interaction::Interaction,
) {
    use crate::interaction::WorldTooltip;
    let Some(call) = interaction.take_world_tooltip() else {
        return;
    };
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

fn dispatch_object_panel_notice(
    shell: Option<&mut crate::ui::UiShell>,
    hud: &mut crate::hud::Hud,
    inter: &mut crate::interaction::Interaction,
    world: &mut dereth_client_model::World,
    notice: &dereth_client_model::Notice,
) {
    let Some(shell) = shell else { return };
    let Some(screen) = crate::hud_drive::game_screen(&mut shell.flow) else {
        return;
    };
    let boundary = shell.ui.requests.len();
    hud.object_notice(&mut shell.ui, screen, world, notice);
    for request in shell.ui.requests.take_since(boundary) {
        if !inter.dispatch_toolbar_query(world, &request) {
            shell.ui.requests.emit(request);
        }
    }
}

/// One host event, routed: a lifecycle event to the runtime's window procedure (and its
/// messages on to the input manager, which releases every held control on a focus loss), a
/// device event through the residual flycam and mouse-look latches, the window procedure's
/// device arms (the Alt+Enter toggle among them) and the input manager.
///
/// A close request is always honoured, Alt+F4's included: the window procedure lets Alt+F4 through
/// to the default procedure, whose answer is the close.
fn route_host_event<H: Host>(
    core: &mut CoreApp<H>,
    shell: &mut ClientShell<H>,
    event: &crate::platform::window::HostEvent,
    time_ms: u32,
) {
    use crate::platform::window::HostEvent;

    if let Some(lifecycle) = crate::platform::window::lifecycle(event) {
        core.handle_window_event(shell, &lifecycle, time_ms);
        if let Some(input) = shell.input.as_mut() {
            // `wnd_proc_disposition` inside the input manager applies the window procedure's
            // table again and drops everything it does not forward.
            for m in dereth_client_runtime::pump::Pump::map_window_event(&lifecycle) {
                input.on_message(crate::pump::from_window(m, time_ms));
            }
        }
        return;
    }
    match event {
        HostEvent::KeyboardInput { key, pressed, .. } => flycam_key(core, shell, *key, *pressed),
        HostEvent::MouseInput {
            button: crate::platform::keys::MouseButton::Right,
            pressed,
        } => core.mouse_look_button(*pressed),
        HostEvent::CursorMoved { x, y } => core.cursor_moved(*x, *y),
        _ => {}
    }
    // The window procedure runs its message table, then packages
    // `hwnd/message/wParam/lParam/GetMessageTime()` into a `MSG` for the input manager's
    // message handler, whose tap and double-click thresholds are driven by that time.
    for m in shell.devices.map_device_event(event, time_ms) {
        core.pump
            .dispatch(crate::pump::window_message(m), m.time_ms);
        if let Some(input) = shell.input.as_mut() {
            input.on_message(m);
        }
    }
}

/// The residual flycam's two keys. See [`App::flycam_key`].
fn flycam_key<H: Host>(
    core: &mut CoreApp<H>,
    shell: &ClientShell<H>,
    key: crate::platform::keys::Key,
    down: bool,
) {
    use crate::platform::keys::Key;
    if shell
        .input
        .as_ref()
        .is_some_and(crate::input::InputShell::keyboard_blocked)
    {
        return;
    }
    if key == Key::SPACE {
        core.flycam_rise(down);
    } else if key == Key::KEY_C {
        core.flycam_sink(down);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::host::NullHost;
    use crate::platform::window::HostEvent;

    /// The host's physical resize reaches both the real backbuffer and the UI coordinate space, so
    /// the picture is the window's real pixels at any desktop scaling (retail runs DPI-unaware and
    /// lets Windows scale it). Client divergence CD-004.
    ///
    /// Behaviour: presentation.window.the-picture-is-the-windows-real-pixels-at-any-desktop-scaling
    #[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn physical_window_resize_reaches_the_backbuffer_and_ui_without_changing_preferences() {
        let mut cfg = Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            dat_dir: dereth_dat::testing::dat_dir(),
            preferences_file: std::env::temp_dir().join("dere-p1-88-not-created/prefs.ini"),
            ..Config::default()
        };
        cfg.display.full_screen = false;
        let mut app = App::<NullHost>::new(cfg).expect("offscreen App with retail UI assets");
        app.start_shell().expect("real UI");
        let preference = app.config().display.resolution;
        let position = app.window_rect();
        for (w, h) in [(1920, 1080), (800, 600)] {
            app.handle_window_event(
                &HostEvent::Resized {
                    width: w,
                    height: h,
                },
                0,
            );
            assert_eq!(
                app.present.size(),
                (w, h),
                "backbuffer follows physical client size"
            );
            assert_eq!(
                app.ui().unwrap().ui.display(),
                (w as i32, h as i32),
                "UI/input space agrees"
            );
            assert_eq!(
                app.config().display.resolution,
                preference,
                "OS events do not save a preference"
            );
            assert_eq!(
                app.window_rect(),
                position,
                "a resize notification does not recenter"
            );
        }
        app.handle_window_event(
            &HostEvent::Resized {
                width: 0,
                height: 0,
            },
            0,
        );
        assert_eq!(
            app.present.size(),
            (800, 600),
            "minimizing does not create a zero buffer"
        );
        assert_eq!(app.ui().unwrap().ui.display(), (800, 600));
    }

    /// A second client starts while the first is still running: startup takes no
    /// one-client-per-machine claim, so the second is not refused and both run their interface.
    ///
    /// Client divergence CD-008.
    ///
    /// Behaviour: presentation.startup.a-second-client-starts-while-the-first-is-running
    #[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_second_client_starts_while_the_first_is_running() {
        let cfg = || Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            dat_dir: dereth_dat::testing::dat_dir(),
            preferences_file: std::env::temp_dir().join("dereth-two-clients-not-created/prefs.ini"),
            ..Config::default()
        };
        let mut first = App::<NullHost>::new(cfg()).expect("the first client starts");
        first.start_shell().expect("the first client's interface");
        let mut second =
            App::<NullHost>::new(cfg()).expect("a second client starts beside the first");
        second
            .start_shell()
            .expect("the second client's interface, with the first still running");
        assert!(first.ui().is_some() && second.ui().is_some());
        drop(second);
        assert!(first.ui().is_some(), "the first client runs on");
    }

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
    #[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
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
