//! Application construction and shell startup.

use super::*;

impl<S: Shell> App<S> {
    /// Run the twelve startup steps.
    ///
    /// The platform and the presentation are the caller's, and they are asked for at the steps
    /// the client builds them: `platform` at step 12, once the data files are open, and
    /// `present` straight after it, handed the window it just produced -- a device needs the
    /// window's handles. A headless caller hands back what it already has.
    ///
    /// # Errors
    /// [`StartupError`] for any of the failures the client treats as fatal.
    pub fn bring_up(
        cfg: Config,
        platform: impl FnOnce(&Config) -> Result<Platform, StartupError>,
        present: impl FnOnce(
            &dyn WindowHost,
            u32,
            u32,
            &Config,
        ) -> Result<Box<S::Present>, StartupError>,
    ) -> Result<Self, StartupError> {
        Self::bring_up_with_store(cfg, None, platform, present)
    }

    /// [`Self::bring_up`] with the data files already open: step 10 takes `store` instead of
    /// opening [`Config::dat_dir`]. A platform with no file the client can open by path (a
    /// browser, which reads the player's files from its own storage) opens them itself. The
    /// world's overlay is laid over `store` as over the files a folder opens, and a later data
    /// patch reopens the overlay over the same `store`, so the patch is read there too.
    ///
    /// # Errors
    /// As [`Self::bring_up`].
    pub fn bring_up_with_store(
        cfg: Config,
        store: Option<std::sync::Arc<dereth_dat::RetailDatStore>>,
        platform: impl FnOnce(&Config) -> Result<Platform, StartupError>,
        present: impl FnOnce(
            &dyn WindowHost,
            u32,
            u32,
            &Config,
        ) -> Result<Box<S::Present>, StartupError>,
    ) -> Result<Self, StartupError> {
        // The client's initialization begins by forcing an 800x600 display size.
        //
        // Display preferences are loaded later in startup, after the force flag is already
        // set — so **the retail client opens its window at 800x600 whatever the profile names**,
        // and the saved resolution only reaches the presentation when gameplay-screen construction
        // lifts the force. Startup does not use the preference directly; it uses the preference
        // *through* that override.
        //
        // The one gate is this build's own. `Config::width`/`height` is also the headless render
        // extent, set directly by test files and capture runs, and a directly-set extent was never
        // display-preference load's answer. So the force only claims a presentation the preference
        // actually produced; see [`App::unforced_resolution`].
        let mut cfg = cfg;
        let unforced_resolution = (cfg.width, cfg.height);
        let forced_resolution = crate::config::FORCED_LOGIN_SIZE;
        let from_preference = cfg
            .load_display_preferences(None, true)
            .is_some_and(|p| (p.width, p.height) == unforced_resolution);
        if from_preference {
            cfg.width = forced_resolution.0;
            cfg.height = forced_resolution.1;
        }
        let forced_resolution = if from_preference {
            forced_resolution
        } else {
            unforced_resolution
        };
        let cfg = cfg;

        // Step 9: initialize networking, plus the connection portion of step 11.
        //
        // Network initialization allocates the client network object, the packet controller, and
        // five receive queues before the database opens because the dat cache uses queue 5. The
        // socket is bound later, when the run path connects before entering its frame loop.
        // Both are done here, because `App::new` is where this crate's fallible startup lives and
        // `App::run` returns a frame count rather than a `Result`. The observable difference is
        // only *when* a bad `-h` is reported, and it is still reported before any frame.
        //
        // A failure is fatal: startup returns the error and the process displays it before exit.
        //
        // **This is the normal path.** The shipped client connects unconditionally and
        // refuses to start without `-a` and `-h`, so it has no offline mode. This build does,
        // for the offline slices and the capture gate, and the way it keeps both is to make the
        // *credentials* the switch: given an account and a host, the client connects, exactly as
        // the launcher's child process does; given neither, it draws the world with no server.
        // `--no-connect` is the explicit opt-out, and giving one of the two without the other is
        // still the documented error rather than a silent offline start.
        let link = if cfg.will_connect() {
            cfg.require_account_and_host().map_err(|e| {
                // The dialog shows corestrings 205 whatever went wrong and discards accumulated
                // connection text, so the detail goes to the log, which
                // is where this rebuild's equivalent of retail's full output text lives.
                tracing::error!("{}", e.detail);
                StartupError::CommandLine(e.to_string())
            })?;
            // The connection sequence number is the low 32 bits of the current real-time
            // millisecond count. ACE reads it as `Timestamp` and otherwise ignores it.
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: the deliberate 32-bit wrap of a millisecond counter, not a float conversion.
            let seq = (crate::platform::clock::system_unix_time().map_or(0, |d| d.as_millis())
                & u128::from(u32::MAX)) as u32;
            let client_port = u16::try_from(cfg.client_port).unwrap_or(0);
            let mut link = NetLink::connect(
                &cfg.host,
                u16::try_from(cfg.port).unwrap_or(0),
                client_port,
                &cfg.account,
                &cfg.vg_password,
                seq,
            )?;
            // Before the first login request goes: a world whose server wants another logon
            // version is sent that one.
            link.net.set_logon_version(&cfg.logon_version);
            Some(link)
        } else {
            None
        };
        // A headless client with no server answers for the server: see `crate::server_stub`.
        let server_stub =
            (link.is_none() && cfg.headless).then(crate::server_stub::ServerStub::default);

        // Step 10: open the data files. Files the platform opened carry the world's overlay as
        // files opened from a folder do; they are kept as the base a reopen lays it over again.
        let (store, base_store) = match store {
            Some(base) => {
                let laid = crate::world_overlay::lay_over((*base).clone(), &cfg);
                let store = if laid.has_overlay() {
                    std::sync::Arc::new(laid)
                } else {
                    std::sync::Arc::clone(&base)
                };
                (store, Some(base))
            }
            None => (std::sync::Arc::new(crate::assets::open_store(&cfg)?), None),
        };

        // Step 12: UI initialization -> (windowed, title, 800, 600, visible, "").
        // The window and its metrics; `Platform` is supplied by a headless caller
        // and built here otherwise. Window creation lives in
        // [`crate::platform::window::open_window`].
        // The three platform seams, taken here because this is step 12 --
        // and the clock's epoch begins here rather than at an earlier startup step.
        // `Platform::open` opens the window first and takes the clock after it.
        let Platform {
            window,
            clock,
            pacer,
            dialog,
        } = platform(&cfg)?;
        let (client_w, client_h) = window.client_size();
        // A headless run has no window, so the rectangle it would have had is
        // seeded from the normal creation placement over the headless screen. That is
        // the *retail* placement — the screen centre — and it is deliberately not the divergence:
        // the divergence is about not moving a window that already exists, and a window being
        // created has no position to keep.
        // `false`, not `cfg.display.full_screen`: the window is created windowed whatever the
        // preference says, and full screen is applied once a screen that allows it is shown. See
        // [`Self::follow_gameplay_full_screen`] and `platform::window::open_window`.
        let frame_metrics = window.frame_metrics().unwrap_or((0, 0, 0));
        let window_rect = if window.has_window() {
            window.window_rect()
        } else {
            let seed = dereth_client_contract::window_proc::placement(
                false,
                true,
                i32::try_from(cfg.width).unwrap_or(i32::MAX),
                i32::try_from(cfg.height).unwrap_or(i32::MAX),
                &window.screen_metrics(frame_metrics),
            );
            Some(dereth_client_contract::window_proc::Rect {
                left: seed.x,
                top: seed.y,
                right: seed.x + seed.cx,
                bottom: seed.y + seed.cy,
            })
        };

        // Step 11 of graphics-engine startup, after which the client is ready.
        //
        // Only when the caller did not bring its own presentation. The device is
        // built here rather than by [`App::bring_up`] because it needs the window handles the block
        // above has just produced.
        let present = present(&*window, client_w, client_h, &cfg)?;

        let timer = Clock::init();

        let mut pump = Pump::new();
        pump.state.is_ready = true;
        // The full-screen preference, which is what Alt+Enter flips
        // (the event loop's epilogue) and what
        // render-preference polling compares against its shadow every frame.
        //
        // **It starts `false` regardless of the preference.** The preference
        // lives in `cfg.display.full_screen` and is what the options page writes;
        // it is applied once a screen that allows full screen is shown. Seeding the
        // shadow from the preference here would make the first frame full screen and then make
        // [`Self::follow_gameplay_full_screen`] undo it at a screen held at the login size, which
        // is a visible flash rather than a no-op. See that function for the whole rule.
        //
        // `allow_full_screen_mode` starts `false` for the same reason: no screen that allows it
        // is shown yet. [`Self::do_event_loop`] maintains it from the second line of every frame.
        pump.state.allow_full_screen_mode = false;
        // A headless run has no window to lose focus, so it is always the foreground application.
        // That matters: the frame pacer would otherwise cap every frame at 99 ms.
        pump.state.is_active_app = cfg.headless;
        pump.state.is_minimized = !cfg.headless;

        // Read before the struct literal because `timer` is moved into it.
        let clock_start = timer.cur_time;
        // For the same reason: `cfg` is moved into it. `false` for the reason the
        // pump shadow above is.
        let started_full_screen = false;
        let started_sync_to_refresh = cfg.display.sync_to_refresh;

        // Built before the struct literal because `store` is moved into it.
        let anim_assets = std::sync::Arc::new(dereth_world_data::anim_assets::DatAnimAssets::new(
            std::sync::Arc::clone(&store),
        ));

        let objects = crate::objects::ObjectStream::with_store(std::sync::Arc::clone(&store));
        // Before the struct literal, for the same reason `anim_assets` is: `cfg`
        // is moved into it.
        // Every patch goes into the world's overlay, never into the locked files.
        let ddd = crate::world_overlay::patcher(&store, &cfg);
        // The object identity verdicts start now, whatever look is chosen, so a later switch to
        // the other era's look is instant.
        let scene_cfg = cfg.scene_config();
        let object_identity = scene_cfg.object_identity_budget.and_then(|budget| {
            ObjectIdentityPrep::start(&store, budget, scene_cfg.object_identity_cache)
        });
        #[cfg(feature = "hifi")]
        let hifi_interface = cfg.render.fidelity.interface;
        Ok(Self {
            cfg,
            state: AppState::Startup,
            pump,
            timer,
            clock,
            pacer,
            events: FrameEvents::new(),
            present,
            world: None,
            store,
            taboo_table: None,
            window,
            input: CameraInput::default(),
            char_input: crate::character::CharacterInput::default(),
            movement: crate::character::MovementCommands::default(),
            mouse_look: false,
            orbit: None,
            orbit_keys: crate::orbit::MovementKeys::default(),
            orbit_pending: Vec::new(),
            #[cfg(feature = "hifi")]
            hifi_interface,
            #[cfg(feature = "hifi")]
            fidelity_told: Vec::new(),
            orbit_look: (0.0, 0.0),
            smooth_animation: false,
            smooth_movement: false,
            press_attacks: false,
            last_mouse_move: 0.0,
            last_cursor: None,
            last_time: 0.0,
            last_target_tracking: None,
            last_option_environment: None,
            environment_override: crate::environment::EnvironmentOverrideState::default(),
            link,
            server_stub,
            script: EnterWorldScript::default(),
            last_link_status: None,
            last_rejected: 0,
            last_net_error: None,
            dialog,
            connect_failure: None,
            objects,
            pending_scene: None,
            scene_config: None,
            backdrop: None,
            last_viewer_block: None,
            last_viewer_cell: None,
            last_object_report: None,
            viewer_block_gate: crate::report_gate::ReportGate::default(),
            object_report_gate: crate::report_gate::ReportGate::default(),
            unowned_gate: crate::report_gate::ReportGate::default(),
            unowned_suppressed: 0,
            scripted_preferences: Vec::new(),
            scripted_actions: Vec::new(),
            frames_begun: 0,
            perf: crate::perf::FramePerf::default(),
            perf_steps: [0.0; crate::frame::STEPS],
            perf_font: false,
            playable_at: None,
            // The reporter seeds `last_sent_position_time` with the clock start, not zero.
            position: dereth_client_net::client_session::PositionReporter::new(clock_start),
            last_jump_request: None,
            interaction: crate::interaction::Interaction::new(),
            dialogs: crate::dialogs::DialogService::default(),
            resolution: crate::resolution::ResolutionTransaction::default(),
            actions: crate::actions::ActionQueue::default(),
            audio: None,
            host_state: HostState::default(),
            pending_ddd: Vec::new(),
            ddd,
            ddd_invalidation: None,
            store_generation: 0,
            base_store,
            pending_auto_layout: false,
            duties: FrameDuties::default(),
            applied_full_screen: started_full_screen,
            applied_resolution: (client_w, client_h),
            applied_sync_to_refresh: started_sync_to_refresh,
            // True before a single preference has been read.
            use_forced_resolution: true,
            forced_resolution,
            unforced_resolution,
            frame_metrics,
            window_rect,
            windowed_rect_before_full_screen: None,
            startup_sound_played: false,
            ui_sound_table: None,
            hud: S::Hud::default(),
            teleport: crate::teleport::Teleport::new(),
            anim_assets,
            object_identity,
        })
    }

    /// The asset seam, which is all any other consumer ever needs.
    #[must_use]
    pub fn assets(&self) -> &dyn dereth_primitives::AssetSource {
        &*self.store
    }

    /// Load the static scene — one landblock's terrain and scenery, plus the LOD window
    /// around it — and point the free camera at it.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the region, the landblock or a device resource is unavailable.
    pub fn load_static_scene(
        &mut self,
        cfg: crate::scene::SceneConfig,
    ) -> Result<(), StartupError> {
        // The switches this scene was built from are kept so the world can be
        // built a *second* time; see [`App::scene_config`].
        self.scene_config = Some(cfg);
        self.present
            .load_world(&self.store, cfg, &mut self.world)
            .map_err(|e| StartupError::Device {
                cause: format!("{e}"),
            })?;
        if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
            world.set_environment_override_state(self.environment_override.clone());
        }
        Ok(())
    }

    /// Start input, sound, and the UI shell in the client's order.
    ///
    /// Split out of [`App::bring_up`] because only the first is fatal and because the two offline
    /// slices and the headless capture gate must keep running with none of them.
    /// Startup order is *data files* → *network* → *keymap* → *UI*, and sound comes up after
    /// the dat cache; the keymap is loaded before the UI
    /// because a text element registers input maps as soon as a screen builds one.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the UI's own two dat objects are missing; without them no
    /// screen can be reached.
    pub fn start_shell(&mut self, shell: &mut S) -> Result<(), StartupError> {
        // The front end's device input first: a client with no key bindings is useless but can
        // still draw, so a front end that cannot load its bindings says so and carries on.
        shell.start_input(&mut UiContext::new(self));

        // A headless run never takes the machine's audio endpoint.
        let want_device = self.cfg.sound && !self.cfg.headless;
        // Seed the random stream from `time(NULL)`. Called once immediately after timer
        // initialization and unconditionally, i.e.
        // **not** gated on DirectSound the way the CRT `srand` is.
        //
        // **A headless run seeds it with 1 instead**, for the same reason `Clock::fixed_step`
        // exists and by the same rule: `--headless` is a regression harness and its output must be
        // reproducible. The seed decides which row of a multi-row sound table
        // entry plays and *when* each ambient fires, so
        // with the wall clock in it two runs of the same frame report a different number of live
        // voices — which is what the headless acceptance gate compares. The windowed path reads the
        // clock, exactly as the client does; nothing but the headless harness is affected, and the
        // draw order within a seed is unchanged.
        let seed = if self.cfg.headless {
            1
        } else {
            crate::audio::ran2_seed(&*self.clock)
        };
        // The eight `Sound.*` preferences the `UserPreferences.ini` carries, rather than
        // `Prefs::default()`. The shipped client loads user preferences before the sound
        // manager registers anything, so file values are already present when sound starts.
        let sound_prefs = crate::audio::prefs_from_file(&self.cfg.preferences_file);
        let mut audio = crate::audio::Audio::new(sound_prefs, seed, want_device);
        // **The wizard's opening roll comes out of these two seeds.**
        // Character randomization draws the heritage and gender from `ran2`, the same stream
        // sound uses, and everything else from CRT `rand()`. The CRT stream
        // is seeded here or not at all -- sound initialization calls `srand`
        // only when DirectSound came up. A headless, silent run is therefore (1, 1) and its roll
        // is reproducible.
        self.host_state.chargen_seeds = Some((seed, audio.crt_seed()));
        if audio.has_device() {
            tracing::info!("audio device at {} Hz", audio.device_rate());
        }
        // The one sound that proves the path: the UI button-press sound (0x72) out of the UI sound
        // table resolves, through entry 7
        // (play a sound-table row from the centre).
        if let Some(table) = crate::assets::enum_did(
            &*self.store,
            crate::audio::UI_SOUND_TABLE_GROUP,
            crate::audio::UI_SOUND_TABLE_ENUM,
        ) {
            if audio.load_sound_table(&self.store, table) {
                // Loading a table creates every non-zero sound referenced by its recursively
                // unpacked nodes. The UI table is not merely the
                // button-click row: the same resident object supplies portal and Admin_Environs
                // sounds. Loading only the click row would make those other producers reach a
                // valid table selection and then mix silence because their waves do not exist.
                audio.create_table_waves(&self.store, table);
                self.ui_sound_table = Some(table);
            }
        }
        self.audio = Some(audio);

        // Initialize the UI after input and sound.
        if self.cfg.ui {
            shell.start_ui(&mut UiContext::new(self))?;
            // The preference store, if the front end did not start it at its own point.
            if !self.duties.preferences_started {
                dereth_client_contract::options::store::init();
                self.start_preferences();
            }
        }
        // `Attribute2ndTable 0x0E000003`, which the vitals bar needs
        // for every maximum it shows.
        // The server's era, when the launcher passed it, wins over the one the data files suggest.
        if let Some(era) = self.cfg.era {
            self.hud.era.era = era;
            self.hud.era.era_announced = true;
        }
        // And so do the systems it announces for its world, each over the era's table.
        self.hud.era.announced_features = self.cfg.era_features;
        // And the rules the world's own client played by, which every front end and the object
        // model read from here.
        if let Some(name) = &self.cfg.world_rules.profile {
            tracing::info!(
                "playing by the world profile {name}: burden Strength {:+}, run scale {}, jump \
                 scale {}",
                self.cfg.world_rules.burden_strength_bonus,
                self.cfg.world_rules.run_scale,
                self.cfg.world_rules.jump_scale
            );
        }
        self.hud.era.world_rules = self.cfg.world_rules.clone();
        self.objects.world.world_rules = self.cfg.world_rules.clone();
        self.host_state.world_rules = self.cfg.world_rules.clone();
        self.hud.load_tables(&self.store, &self.objects.world);
        // The same table computes the maximum a received current vital is clamped to, which the
        // world's quality-update paths apply before storing.
        if let Some(t) = self.hud.vitals_table {
            self.objects.world.install_vital_formulas(t);
        }
        // The maximum is enchanted only as far as the quality filter allows.
        if let Some(f) = self.hud.quality_filter.clone() {
            self.objects.world.install_quality_filter(f);
        }
        // Inventory refusal text is built by `World`, but its material names are
        // process-owned DAT content loaded here with the rest of the HUD's enum mappers. Resolve
        // them through the one material-type-to-string transcription and give the model strings,
        // not a second table decoder or material-name formatter.
        let material_names = self.hud.material_names.as_ref().map_or_else(
            std::collections::BTreeMap::new,
            |names| {
                names
                    .enum_to_name
                    .iter()
                    .filter_map(|(id, _)| {
                        crate::hud::material_name_of(Some(names), *id).map(|name| (*id, name))
                    })
                    .collect()
            },
        );
        self.objects.world.install_material_names(material_names);
        // The enum lookup `(0x11, 2, 0x14)` resolves to the shipped TabooTable `0x0E00001E`.
        // Load it once beside the other UI-owned tables; the option itself is synchronized at the
        // head of every HUD event batch because a world reset replaces `Scroll`.
        let taboo_id = DataId(0x0E00_001E);
        self.taboo_table = self
            .store
            .read(taboo_id)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                <dereth_assets::TabooTable as dereth_assets::Decode>::decode_payload(
                    taboo_id, &bytes,
                )
                .map_err(|e| e.to_string())
            })
            .map(std::sync::Arc::new)
            .map_err(|e| tracing::warn!("TabooTable: {e}"))
            .ok();
        // The enum lookup `(7, 2, 0x11)` selects the `ChatPoseTable`, registered by
        // the client and fetched as chat-pose lookup's first act. Loaded once, beside the
        // HUD's own tables, because `Interaction` has no dat store: it is the table that turns
        // `*wave*` into a motion, a `0x01E1` and a local echo, and the pose lookup's
        // `if (!table) return false` is exactly what a miss here reproduces.
        if self.interaction.chat_pose_table.is_none() {
            match self
                .store
                .ids_of(dereth_dat::DbType::ChatPoseTable)
                .into_iter()
                .next()
                .ok_or_else(|| "client_portal.dat carries no ChatPoseTable".to_owned())
                .and_then(|id| {
                    self.store
                        .read(id)
                        .map_err(|e| e.to_string())
                        .and_then(|b| {
                            <dereth_assets::tables::ChatPoseTable as dereth_assets::Decode>::decode_payload(id, &b)
                                .map_err(|e| e.to_string())
                        })
                }) {
                Ok(t) => self.interaction.chat_pose_table = Some(std::sync::Arc::new(t)),
                Err(e) => tracing::warn!("ChatPoseTable: {e}"),
            }
        }
        Ok(())
    }

    /// Start the preference store the options pages read, over a registry that holds its
    /// registered defaults: the display modes the adapter offers, and then the player's saved
    /// `UserPreferences.ini` over the defaults.
    ///
    /// A front end that rebuilds the registry itself runs this straight after; otherwise
    /// [`Self::start_shell`] registers the defaults and runs it once the UI is up. The display
    /// choices go before the file because a saved resolution resolves through their label list.
    ///
    /// A player whose file names no interface (a new player, or one whose file came from the
    /// original game) starts in the configuration's new-player interface, when it names one.
    pub fn start_preferences(&mut self) {
        use dereth_client_contract::options::{interface, store};
        self.duties.preferences_started = true;
        self.register_display_modes();
        let ini = crate::platform::files::read_to_string(&self.cfg.preferences_file)
            .ok()
            .and_then(|t| {
                dereth_client_contract::persist::preferences::UserPreferences::parse(&t).ok()
            });
        match &ini {
            Some(ini) => {
                let (applied, ignored) = store::load(ini);
                tracing::debug!("user preferences: {applied} applied, {ignored} for other owners");
            }
            // "A missing file is not an error": retail ignores the preference loader's result,
            // and the registered defaults are then what the page shows.
            None => tracing::info!("no UserPreferences.ini; registered defaults stand"),
        }
        if let Some(first) = self.cfg.new_player_interface {
            if interface::start_new_player(ini.as_ref(), first) {
                tracing::info!(
                    "the preferences name no interface: starting in the {} interface",
                    first.label()
                );
            }
        }
    }

    /// Play the one sound the bring-up proves the device with, once.
    ///
    /// The UI button-press sound (`0x72`) through entry 7, which is the path the UI
    /// `MediaPlayback`'s sound media descriptor takes for every button in the game.
    pub fn play_startup_sound(&mut self) {
        if self.startup_sound_played {
            return;
        }
        let Some(table) = self.ui_sound_table else {
            return;
        };
        let Some(audio) = self.audio.as_mut() else {
            return;
        };
        audio.play_ui_sound(dereth_audio::UiSoundRef::Table {
            table,
            stype: crate::audio::SOUND_UI_BUTTON_PRESS,
        });
        self.startup_sound_played = true;
    }
}
