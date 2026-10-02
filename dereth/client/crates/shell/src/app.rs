//! The executable's front end: the retail UI, the cursor, the clipboard, the preview spaces and
//! the overlay, plugged into [`dereth_client_runtime::app::App`]'s frame.
//!
//! The frame loop is the runtime's. What is here is [`App`], the application with this executable's
//! front end ([`ClientShell`], in [`crate::front_end`]) in it: the runtime's application beside
//! its shell, so each frame hands the one to the other. `App` dereferences to the runtime's
//! application, so every read and every step that needs no UI is the runtime's own method.
//!
//! The main loop is tiny: connect once, then run one frame at a time until a frame asks to stop.
//! [`App::run`] is that loop and [`App::frame`] is one frame of it.

pub use dereth_client_runtime::app::*;

use dereth_primitives::{AssetSource, DataId};

use crate::front_end::{flycam_key, route_host_event};
pub use crate::front_end::{ClientShell, KeyBindingStats};
use crate::platform::host::Host;
use crate::{config::Config, present::ClientPresentation};

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
        route_host_event(&mut self.core.ui_context(), &mut self.shell, event, time_ms);
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
        flycam_key(&mut self.core.ui_context(), &self.shell, key, down);
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
}
