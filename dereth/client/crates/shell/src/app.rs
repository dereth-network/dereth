//! The executable's front end: the modern UI, the cursor, the clipboard, the preview spaces and
//! the overlay, plugged into [`dereth_client_runtime::app::App`]'s frame.
//!
//! The frame loop is the runtime's. What is here is [`App`], the application with this executable's
//! front end ([`ClientShell`], in [`crate::front_end`]) in it: the runtime's application beside
//! its shell, so each frame hands the one to the other. `App` dereferences to the runtime's
//! application, so every read and every step that needs no UI is the runtime's own method.
//!
//! The main loop is tiny: connect once, then run one frame at a time until a frame asks to stop.
//! [`App::run`] is that loop and [`App::frame`] is one frame of it.

#[cfg(gpu)]
use dereth_scene::world_scene::SceneReads;

use dereth_client_runtime::app::*;

use dereth_primitives::{AssetSource, DataId};

use crate::front_end::{flycam_key, route_host_event};
pub use crate::front_end::{ClientShell, KeyBindingStats};
use crate::platform::host::Host;
use {crate::present::ClientPresentation, dereth_client_runtime::config::Config};

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
    /// Preferences were loaded by [`Config::from_args_and_prefs_at`]; database
    /// initialization and UI initialization run in the documented order,
    /// because the database must precede the UI (the UI layouts are dat objects).
    ///
    /// # Errors
    /// [`StartupError`] for any of the failures the client treats as fatal.
    #[cfg(gpu)]
    pub fn new(cfg: Config) -> Result<Self, StartupError> {
        Self::bring_up(cfg, None, None)
    }

    /// The same bring-up with the presentation supplied.
    ///
    /// This is the ungated constructor: it is what a headless `App` is built with
    /// ([`dereth_client_runtime::present::NullPresentation`]), and it is what a second backend is handed to.
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
                #[cfg(gpu)]
                None => Self::device_presentation(window, client_w, client_h, cfg),
                #[cfg(not(gpu))]
                None => {
                    let _ = (window, cfg);
                    Ok(Box::new(
                        dereth_client_runtime::present::NullPresentation::new(client_w, client_h),
                    ))
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
            &dyn dereth_client_runtime::platform::window::WindowHost,
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

    /// Run the normal cleanup path, in [`dereth_client_runtime::shutdown::Step::ORDER`].
    pub fn shutdown(self) -> dereth_client_runtime::shutdown::CleanupLog {
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
    #[cfg(any(test, feature = "test-support"))]
    pub fn apply_interaction_events_to_panels(
        &mut self,
        events: &[dereth_client_net::client_session::SessionEvent],
    ) {
        self.core
            .probe_mut()
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
        cfg: dereth_client_runtime::scene::SceneConfig,
    ) -> Result<(), StartupError> {
        self.core.load_static_scene(cfg)
    }

    /// One host event, as the event loop consumes it: a lifecycle event reaches the runtime's
    /// window procedure, a device event this client's input.
    pub fn handle_window_event(&mut self, event: &dereth_input::host::HostEvent, time_ms: u32) {
        route_host_event(&mut self.core.ui_context(), &mut self.shell, event, time_ms);
    }

    /// Queue a host event as the window would; the next frame routes it to whichever interface
    /// is shown. For in-process drivers and tests.
    pub fn queue_window_event(&mut self, event: dereth_input::host::HostEvent) {
        self.shell.queue_window_event(event);
    }

    /// The device input, for the tests and the console's state line.
    pub fn input_manager_mut(&mut self) -> Option<&mut crate::input::InputShell> {
        self.shell.shared.input.as_mut()
    }

    /// The device input, read-only.
    #[must_use]
    pub fn input_manager(&self) -> Option<&crate::input::InputShell> {
        self.shell.shared.input.as_ref()
    }

    /// One key transition of the residual flycam: `Space` raises it and `C` lowers it, unless a
    /// keyboard barrier stands (a focused text box stops these two keys for the same reason it
    /// stops every other one). Every other key is not the flycam's.
    pub fn flycam_key(&mut self, key: dereth_input::keys::Key, down: bool) {
        flycam_key(&mut self.core.ui_context(), &self.shell, key, down);
    }

    /// The runtime's own focus-loss latches, for a lifecycle event; a device event has none.
    pub fn note_flycam_input(&mut self, event: &dereth_input::host::HostEvent) {
        if let Some(lifecycle) = crate::platform::window::lifecycle(event) {
            self.core.note_flycam_input(&lifecycle);
        }
    }

    /// For the gated [`App::new`] path: construct the graphics engine and apply the three
    /// presentation preferences it needs before anything draws.
    #[cfg(gpu)]
    fn device_presentation(
        window: &dyn dereth_client_runtime::platform::window::WindowHost,
        client_w: u32,
        client_h: u32,
        cfg: &Config,
    ) -> Result<Box<dyn ClientPresentation>, StartupError> {
        let handles = window.render_handles();
        let mut renderer = Self::device_on_selected_backend(handles, client_w, client_h, cfg)?;
        // Startup registers the render variable over preloaded preference shadows. Apply
        // the configured profile before any world/UI draw; no owner settings are written.
        renderer.set_texture_filtering(
            dereth_client_runtime::render_prefs::texture_filtering_from_file(&cfg.preferences_file),
        );
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
    #[cfg(gpu)]
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
        let (wanted, hifi) = startup_device(cfg, fallback, cfg!(feature = "hifi"));
        let make = |backend: Backend| {
            // Asked for only on the device that can draw it.
            let hifi = hifi && backend == Backend::Wgpu;
            #[cfg(feature = "hifi")]
            {
                crate::gpu::Renderer::new_on_for_hifi(backend, handles, client_w, client_h, hifi)
            }
            #[cfg(not(feature = "hifi"))]
            {
                let _ = hifi;
                crate::gpu::Renderer::new_on(backend, handles, client_w, client_h)
            }
        };

        match make(wanted) {
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
                let r = make(fallback).map_err(|e| StartupError::Device {
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
        let id = DataId(dereth_client_runtime::assets::FIRST_PIXEL_SURFACE);
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
        if let Some(shell) = self.shell.modern.ui.as_mut() {
            shell.queue(m);
        }
    }

    /// The UI-flow shell, for the tests and for the report line.
    #[must_use]
    pub fn ui(&self) -> Option<&crate::ui::UiShell> {
        self.shell.modern.ui.as_ref()
    }

    /// The UI-flow shell, mutably, so a test can drive a documented transition.
    pub fn ui_mut(&mut self) -> Option<&mut crate::ui::UiShell> {
        self.shell.modern.ui.as_mut()
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
        let shell = self.shell.modern.ui.as_mut()?;
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
        self.shell.modern.key_binding_stats
    }

    /// This frame's 2D blit list, for the tests.
    #[must_use]
    pub fn ui_draw_list(&self) -> &[dereth_ui::UiDrawCmd] {
        &self.shell.shared.ui_draw_list
    }

    /// What the cursor path did this session — [`crate::cursor::CursorStats`].
    ///
    /// `updates` is the per-frame wire: it must equal the number of frames drawn.
    #[must_use]
    pub fn cursor_stats(&self) -> crate::cursor::CursorStats {
        self.shell.shared.cursor.stats
    }

    /// The cursor asset currently applied, exposed for tests.
    #[must_use]
    pub fn current_cursor_did(&self) -> Option<dereth_primitives::DataId> {
        self.shell.shared.cursor.current()
    }

    /// Access to the renderer, for the capture the acceptance gate takes.
    ///
    /// `App` holds a [`dereth_client_runtime::present::Presentation`], so this is a downcast of it and is gated on the device feature exactly as the renderer itself is. It panics if the
    /// presentation is not the device one, which is the same contract the field access had: a
    /// caller of this function has already decided it is driving a real device.
    #[cfg(gpu)]
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
    pub fn chargen_dress(&self) -> dereth_scene::preview::ChargenDressStats {
        self.shell.modern.chargen_dress
    }

    /// What the UI-texture releases have done, for the descriptor-bound assertion.
    #[must_use]
    pub fn ui_release_report(&self) -> crate::gpu::UiReleaseReport {
        self.shell.shared.ui_release
    }

    /// Read-only access to the renderer, for the counters the report prints.
    ///
    /// See [`App::renderer_mut`] for why this is a downcast and what it asserts.
    #[cfg(gpu)]
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
    #[cfg(gpu)]
    #[must_use]
    pub fn world_scene(&self) -> Option<dereth_scene::world_scene::WorldSceneRef<'_>> {
        let draw = self.renderer().world()?;
        Some(dereth_scene::world_scene::WorldSceneRef {
            world: self.core.world.as_ref()?,
            draw,
        })
    }

    /// …and writable, both halves.
    #[cfg(gpu)]
    pub fn world_scene_mut(&mut self) -> Option<dereth_scene::world_scene::WorldSceneMut<'_>> {
        let draw = self
            .core
            .present
            .as_any_mut()
            .downcast_mut::<crate::gpu::Renderer>()
            .expect("App::world_scene_mut on a presentation that is not the device")
            .world_mut()?;
        Some(dereth_scene::world_scene::WorldSceneMut {
            world: self.core.world.as_mut()?,
            draw,
        })
    }
}

#[cfg(gpu)]
impl<H: Host> App<H> {
    /// Report the static scene immediately after its load completes.
    pub fn log_load_report(&self, scene: dereth_client_runtime::scene::SceneConfig) {
        let descriptors = (
            self.renderer().descriptor_usage(),
            self.renderer().descriptor_stats(),
        );
        if let Some(world) = self.world_scene() {
            let s = world.draw.stats;
            tracing::info!(target: "dereth_client",
                "landblock 0x{:04X}, {} blocks, {} terrain surfaces",
                scene.landblock,
                s.blocks_meshed,
                s.terrain_surfaces
            );
            tracing::debug!(target: "dereth_client",
                "{} scenery + {} buildings + {} statics, {} batches ({} untextured), \
                 {} triangles, {} KiB of dynamic upload per frame",
                s.scenery_objects,
                s.buildings,
                s.static_objects,
                s.object_batches,
                s.object_batches_untextured,
                s.object_triangles,
                s.upload_bytes / 1024
            );
            // "The sky is not black" is a claim about a number, so it is printed.
            let (year, day, t) = world.game_time();
            let l = world.landscape_lighting();
            let sky = world.draw.stats.sky_stats;
            tracing::debug!(target: "dereth_client",
                "year {year} day {day}, time of day {t:.3}; sky {}/{} gfx ids drawable, {} live ({} pass 0, {} pass 1), {} batches, {} triangles, {} missing",
                sky.gfx_ids_drawable,
                sky.gfx_ids,
                sky.live_objects,
                sky.pass0_objects,
                sky.pass1_objects,
                sky.batches,
                sky.triangles,
                sky.missing_geometry
            );
            // The descriptor allocator reuses freed slots, so what is worth printing is the
            // allocator's own view. `live` is what is held right now, `high_water` the largest
            // `live` ever reached, and `frontier` how much fresh heap space was ever taken --
            // `frontier` well below `high_water + free` is the reuse working.
            let (d, ds) = descriptors;
            tracing::debug!(target: "dereth_client",
                "{} object/sky texture(s) + {} merged land surfaces; descriptors live {} / high water {} / frontier {} of {} (free {}, pending {}), {} reuses, {} exhaustions",
                s.textures_uploaded,
                s.terrain_surfaces,
                d.live,
                d.high_water,
                d.frontier,
                d.capacity,
                d.free,
                d.pending,
                ds.reuses,
                ds.exhaustions
            );
            tracing::debug!(target: "dereth_client",
                "ambient {:.3} {:?}, sunlight {:?} (|v| = dir_bright = {:.3}) {:?}",
                l.ambient_level,
                l.ambient_color,
                l.sunlight,
                l.sunlight.magnitude(),
                l.sunlight_color
            );
            let (cells, inside) = world.env_cell_counts();
            tracing::debug!(target: "dereth_client",
                "{cells} interior cell(s) baked for drawing, {inside} batch(es) in \
                 the viewer's own cell"
            );
            if let Some(c) = world.character.as_ref() {
                tracing::debug!(target: "dereth_client",
                    "{} interior cell(s) resident for physics ({:?})",
                    c.land().resident_cells(),
                    c.land().cell_stats()
                );
                tracing::debug!(target: "dereth_client",
                    "character at {:?} in cell {:#010X}, {} drawable parts, \
                     {} batches, {} triangles",
                    c.position().frame.origin,
                    c.position().cell.0,
                    s.character_parts,
                    s.character_batches,
                    s.character_triangles
                );
            }
        }
    }

    /// Report frame, UI, model, input and audio counters before capture and shutdown.
    pub fn log_run_report(&mut self, frames: u64, cfg_connect: bool, want_ui: bool) {
        tracing::info!(target: "dereth_client", "{frames} frame(s) drawn");

        if cfg_connect {
            // What the server actually put in the world, and what was drawn.
            let s = self.objects().stats;
            tracing::info!(target: "dereth_client",
                "objects -- {} created, {} merged, {} recreated, {} removed, \
                 {} stale instances; {} position updates ({} stale), {} movement buffers \
                 ({} undecodable)",
                s.creates,
                s.merges,
                s.recreates,
                s.removes,
                s.stale_instances,
                s.position_updates,
                s.stale_positions,
                s.movement_updates,
                s.movement_undecodable
            );
            if let Some(world) = self.world_scene() {
                let w = world.draw.stats;
                tracing::info!(target: "dereth_client",
                    "{} object(s) drawn from {} setup(s), {} animated, {} batches, \
                     {} triangles",
                    w.server_objects,
                    w.server_object_setups,
                    w.server_objects_animated,
                    w.server_object_batches,
                    w.server_object_triangles
                );
            }
        }

        // The shell's report line: what it did, in the numbers the tests assert on.
        if let Some(shell) = self.ui() {
            let s = shell.stats;
            tracing::info!(target: "dereth_client",
                "UI -- {} mode switch(es) {:?}, {} create failure(s), \
                 {} unregistered request(s), {} request(s) handled, {} unowned",
                s.mode_switches,
                shell.transitions(),
                s.screen_create_failures,
                s.unregistered_mode_requests,
                s.requests_handled,
                s.requests_ignored
            );
        }
        // The HUD's report line: what it read out of the server and what it did with it.
        {
            let h = self.hud();
            let s = h.stats;
            tracing::info!(target: "dereth_client",
                "HUD -- {} player description(s), {} placement row(s) decoded, \
                 {} window visibility/ies applied, {} vital update(s) ({} unstorable), \
                 {} chat line(s) shown ({} filtered out), {} undecodable; \
                 {} vitals / {} toolbar / {} radar write(s), coords {:?}",
                s.player_desc_applied,
                s.placements_decoded,
                s.placements_applied,
                s.vital_updates,
                s.vital_updates_unstorable,
                s.chat_lines,
                s.chat_lines_dropped,
                s.undecodable,
                s.vitals_written,
                s.toolbar_written,
                s.radar_written,
                h.coords
            );
            for (id, row) in &h.placements.rows {
                tracing::debug!(target: "dereth_client", "HUD placement window {id}: {row:?}");
            }
            // The housing subsystem's whole round trip in one line. If the client never sends
            // `0x021E House_QueryHouse`, the shard never sends either answer, and every counter below
            // stays 0 exactly like a shard with nothing to say. A live run that ends with
            // `0 status / 0 data` means the *request* did not go out, which is a different bug from an
            // unreceived answer.
            tracing::info!(target: "dereth_client",
                "house -- {} status / {} data answer(s) to the login QueryHouse, \
                 {} rent-time ({} applied) / {} rent-payment ({} applied) update(s), \
                 {} restriction update(s) ({} applied)",
                s.house_status_notices,
                s.house_data_notices,
                s.house_rent_time_updates,
                s.house_rent_time_applied,
                s.house_rent_payment_updates,
                s.house_rent_payment_applied,
                s.house_restriction_updates,
                s.house_restrictions_applied
            );
            // The slumlord window's own round trip, beside the login one, because the
            // two are about different houses and fail in different ways. A `0 profile(s)` after using
            // a slumlord means either the use never went out or the shard refused it; a non-zero
            // profile count with `0 opened` would mean the receiver ran and the panel did not.
            tracing::info!(target: "dereth_client",
                "slumlord -- {} house profile(s) received, {} window open(s), \
                 {} payment request(s), {} lord re-quer(ies)",
                s.house_profile_notices,
                self.hud().panels.slumlord.opens,
                self.interaction().stats.house_payments_sent,
                self.interaction().stats.house_lord_queries
            );
        }
        {
            // The world's overlay over the locked data files: where it is, and how many reads its
            // own records and its deletions answered since the files were last opened.
            let s = &self.core.store;
            let files = [
                ("portal", Some(s.portal())),
                ("cell", Some(s.cell())),
                ("local", Some(s.local())),
                ("highres", s.highres()),
            ];
            let read: Vec<String> = files
                .iter()
                .filter_map(|(name, f)| {
                    let l = (*f)?.layer()?;
                    let (served, hidden) = l.reads();
                    Some(format!(
                        "{name} {} record(s), {} deletion(s), {served} read(s) answered, {hidden} hidden",
                        l.records().count(),
                        l.tombstones().len()
                    ))
                })
                .collect();
            tracing::info!(target: "dereth_client",
                "overlay -- {}{}",
                s.overlay_dir()
                    .map_or_else(|| "none".to_owned(), |d| d.path().display().to_string()),
                if read.is_empty() {
                    String::new()
                } else {
                    format!(": {}", read.join("; "))
                }
            );
        }
        {
            // The era the client plays and the systems it takes the world to lack (the server's
            // announcement over the era's table), and what the screens last took away for them.
            let h = self.hud();
            let lacks = |f: dereth_primitives::EraFeatures| {
                f.iter()
                    .filter(|(_, on)| !on)
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            tracing::info!(target: "dereth_client",
                "era -- {} ({}), {} system(s) announced; the world lacks [{}]; the screens hide [{}]",
                h.era.era,
                if h.era.era_announced {
                    "announced"
                } else {
                    "from the data files"
                },
                h.era.announced_features.iter().count(),
                lacks(h.era.features()),
                h.panels
                    .era
                    .applied()
                    .map_or_else(|| "nothing yet".to_owned(), lacks)
            );
        }
        if want_ui {
            let t = self.renderer_mut().ui_stats;
            tracing::info!(target: "dereth_client",
                "UI draw -- {} quad(s), {} image(s) uploaded, {} decode failure(s), \
                 {} skipped, {} clipped away",
                t.quads_drawn,
                t.uploaded,
                t.decode_failures,
                t.skipped_draws,
                t.clipped_away
            );
            // Every UI image is one SRV slot, and a screen gives its slots back when `use_new_mode` destroys it: `freed` is what the screens released
            // and `unknown` is a double release, which must be zero. The gameplay screen alone is 114
            // images, so this is the number that says whether the heap is bounded or merely large.
            let d = self.renderer().descriptor_usage();
            let ds = self.renderer().descriptor_stats();
            let r = self.ui_release_report();
            tracing::info!(target: "dereth_client",
                "UI textures hold {} descriptor slot(s); heap live {} / high water {} / frontier {} of {} (free {}), {} reuse(s), {} exhaustion(s); screens released {} (still linked {}, unknown {})",
                self.renderer().ui_texture_count(),
                d.live,
                d.high_water,
                d.frontier,
                d.capacity,
                d.free,
                ds.reuses,
                ds.exhaustions,
                r.freed,
                r.still_linked,
                r.unknown
            );
        }
        {
            let (offered, handled, no_scan, actions) =
                self.input_manager_mut().map_or((0, 0, 0, 0), |i| {
                    (
                        i.stats.messages_offered,
                        i.stats.messages_handled,
                        i.stats.keyboard_without_scan_code,
                        i.stats.actions_fired,
                    )
                });
            tracing::info!(target: "dereth_client",
                "input -- {offered} message(s) offered, {handled} handled, \
                 {no_scan} without a scan code, {actions} action(s)"
            );
        }
        if let Some(audio) = self.audio_mut() {
            tracing::info!(target: "dereth_client",
                "audio -- device {}, {} wave(s) created, {} decode failure(s), \
                 {} sound(s) started, {} voice(s) still playing, {} under-run(s), \
                 {} block(s) submitted, {} movie track(s), peak {:.4}",
                if audio.has_device() { "up" } else { "silent" },
                audio.stats.waves_created,
                audio.stats.wave_decode_failures,
                audio.stats.sounds_started,
                audio.active_voices(),
                audio
                    .stats
                    .underruns
                    .load(std::sync::atomic::Ordering::Relaxed),
                audio
                    .stats
                    .blocks_filled
                    .load(std::sync::atomic::Ordering::Relaxed),
                audio.stats.movie_tracks_started,
                audio.peak_output()
            );
            let w = audio.world_stats();
            tracing::info!(target: "dereth_client",
                "world audio -- {} hook sound(s), {} unplayable; {} server sound(s), {} unplayable",
                w.triggers,
                w.trigger_misses,
                w.server_sounds,
                w.server_sound_misses
            );
        }
    }
}

/// The backend the device first comes up on, and whether it is asked for what the optional
/// high-fidelity presentation draws with, for `cfg` with `fallback` as the default backend and
/// `presentation_built` saying whether this build has the presentation.
///
/// `Config::renderer` is a `dereth_client_contract::RendererChoice` -- the name a player asked
/// for -- so that `config.rs` can live in `dereth-client-runtime`; this is where the choice meets
/// `Backend`: the switch or preference first, then the default. The presentation never moves the
/// device: the backend is exactly the one a build without it would pick.
///
/// The presentation draws on the `wgpu` device alone, and only under the Horizon interface. A
/// client that starts in Horizon on the `wgpu` device with a box ticked has the device asked for
/// what the presentation draws with; every other start-up -- another backend, another interface,
/// no box ticked, or a build without the presentation -- is asked for nothing more, whatever
/// `[Fidelity]` holds, and a box ticked later takes effect at the next start.
#[cfg_attr(not(gpu), allow(dead_code))]
fn startup_device(
    cfg: &Config,
    fallback: dereth_render::device::Backend,
    presentation_built: bool,
) -> (dereth_render::device::Backend, bool) {
    use dereth_render::device::Backend;
    let backend = cfg.renderer.map(Backend::from).unwrap_or(fallback);
    let widened = presentation_built && backend == Backend::Wgpu && horizon_with_a_box(cfg);
    (backend, widened)
}

/// Whether `cfg` starts in the Horizon interface with a `[Fidelity]` box ticked.
#[cfg(feature = "hifi")]
#[cfg_attr(not(gpu), allow(dead_code))]
fn horizon_with_a_box(cfg: &Config) -> bool {
    cfg.render.fidelity.interface && cfg.render.fidelity.any_stored()
}

/// A build without the presentation reads no `[Fidelity]`.
#[cfg(not(feature = "hifi"))]
#[cfg_attr(not(gpu), allow(dead_code))]
fn horizon_with_a_box(_cfg: &Config) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::host::NullHost;
    use dereth_input::host::HostEvent;

    /// The configuration a start-up with `argv` and the preferences file `prefs` reads.
    #[cfg(feature = "hifi")]
    fn started_with(argv: &[&str], prefs: &[&str]) -> Config {
        let argv: Vec<String> = argv.iter().map(|a| (*a).to_owned()).collect();
        Config::from_args_and_prefs_with(
            &argv,
            &dereth_client_runtime::config::Preferences::parse(&prefs.join("\n")),
        )
        .expect("the start-up configuration parses")
    }

    /// The start-up device is the named renderer or the default whatever `[Fidelity]` holds, so
    /// the effects never move the classic or the modern interface onto another backend. Only a
    /// client that starts in Horizon on the `wgpu` device with a box ticked has the device asked
    /// for what the presentation draws with; with no box ticked, under another interface, on
    /// another backend, or in a build without the presentation, the request is the ordinary one.
    ///
    /// Behaviour: hifi.startup.the-device-is-widened-only-on-wgpu-in-horizon-with-a-box-ticked
    #[test]
    #[cfg(feature = "hifi")]
    fn the_start_up_device_is_never_moved_and_is_widened_only_on_wgpu_in_horizon_with_a_box_ticked()
    {
        use dereth_render::device::Backend;
        let vulkan = Backend::Vulkan;
        let every_box = [
            "[Fidelity]",
            "Lighting=True",
            "Shadows=True",
            "GlobalIllumination=True",
            "AmbientOcclusion=True",
            "Lamps=True",
            "Sky=True",
        ];
        let under = |interface: &str| {
            let mut p = vec!["[UI]".to_owned(), format!("Interface={interface}")];
            p.extend(every_box.iter().map(|s| (*s).to_owned()));
            p
        };
        let refs = |v: &[String]| v.iter().map(String::as_str).collect::<Vec<_>>().join("\n");
        let cfg_for = |argv: &[&str], prefs: &[String]| started_with(argv, &[&refs(prefs)]);
        for built in [false, true] {
            assert_eq!(
                startup_device(&started_with(&[], &[]), vulkan, built),
                (vulkan, false),
                "the default preferences, built={built}"
            );
            for interface in ["Classic", "Modern"] {
                assert_eq!(
                    startup_device(&cfg_for(&[], &under(interface)), vulkan, built),
                    (vulkan, false),
                    "every box under {interface}, built={built}"
                );
                assert_eq!(
                    startup_device(
                        &cfg_for(&["--renderer", "wgpu"], &under(interface)),
                        vulkan,
                        built
                    ),
                    (Backend::Wgpu, false),
                    "every box under {interface} on the named wgpu renderer, built={built}"
                );
            }
            assert_eq!(
                startup_device(
                    &started_with(&[], &["[UI]", "Interface=Horizon"]),
                    vulkan,
                    built
                ),
                (vulkan, false),
                "Horizon with no box ticked, built={built}"
            );
        }
        for built in [false, true] {
            assert_eq!(
                startup_device(&cfg_for(&[], &under("Horizon")), vulkan, built),
                (vulkan, false),
                "Horizon with a box ticked and no renderer named keeps the default, built={built}"
            );
        }
        assert_eq!(
            startup_device(&cfg_for(&[], &under("Horizon")), Backend::Wgpu, false),
            (Backend::Wgpu, false),
            "a build without the presentation"
        );
        assert_eq!(
            startup_device(&cfg_for(&[], &under("Horizon")), Backend::Wgpu, true),
            (Backend::Wgpu, true),
            "Horizon with a box ticked on a default wgpu device"
        );
        assert_eq!(
            startup_device(
                &cfg_for(&["--renderer", "wgpu"], &under("Horizon")),
                vulkan,
                true
            ),
            (Backend::Wgpu, true),
            "Horizon with a box ticked on the named wgpu renderer"
        );
        assert_eq!(
            startup_device(
                &started_with(&["--renderer", "wgpu"], &["[UI]", "Interface=Horizon"]),
                vulkan,
                true
            ),
            (Backend::Wgpu, false),
            "Horizon on the named wgpu renderer, nothing ticked"
        );
        assert_eq!(
            startup_device(
                &cfg_for(&["--renderer", "vulkan"], &under("Horizon")),
                Backend::Wgpu,
                true
            ),
            (vulkan, false),
            "a named renderer the presentation cannot draw on"
        );
    }

    /// The host's physical resize reaches both the real backbuffer and the UI coordinate space, so
    /// the picture is the window's real pixels at any desktop scaling (retail runs DPI-unaware and
    /// lets Windows scale it). Client divergence CD-004.
    ///
    /// Behaviour: presentation.window.the-picture-is-the-windows-real-pixels-at-any-desktop-scaling
    #[cfg(gpu)]
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
            preferences_file: std::env::temp_dir().join("dereth-uncreated-preferences/prefs.ini"),
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
    #[cfg(gpu)]
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
}
