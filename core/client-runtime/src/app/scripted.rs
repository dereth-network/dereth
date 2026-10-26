//! Scripted settings, screenshots, and render feedback.

use super::*;

impl<S: Shell> App<S> {
    /// The world's identity for the travel's store: its name and the server's address, since two
    /// servers may share a name and hold different portals.
    fn travel_world(&self) -> Option<String> {
        self.host_state.world_name.as_ref().map(|n| {
            // `-h` may carry the port itself.
            if self.cfg.host.contains(':') {
                format!("{n}@{}", self.cfg.host)
            } else {
                format!("{n}@{}:{}", self.cfg.host, self.cfg.port)
            }
        })
    }

    /// The travel's frame, with `--nav-proof`'s before it: read where the player is and what
    /// stands in the world, carry the journey on, and do what it asks as a player's keys and
    /// requests. Its keys are this frame's, offered after the frame hook has run, so a hook never
    /// takes them for the player's.
    pub(super) fn travel_use_time(&mut self) {
        if self.nav_proof.is_none() {
            if let Some(spec) = self.cfg.nav_proof {
                if self.cfg.enter_world {
                    self.nav_proof = Some(crate::nav_proof::NavProof::new(
                        spec,
                        self.cfg.nav_prep.clone(),
                    ));
                }
            }
        }
        let published = self
            .published_portals
            .lock()
            .ok()
            .and_then(|mut p| p.take());
        if let Some(text) = published {
            if let Err(e) = self.travel.set_published(&text) {
                tracing::warn!("--nav-published: {e}");
            }
        }
        // Nothing asks for a journey or a plan: the travel keeps its world's knowledge loaded
        // (plugins read it), and the frame gathers nothing for it and builds no planner.
        if self.nav_proof.is_none() && self.cfg.nav_prep.is_empty() && !self.travel.wanted() {
            if let Some(world) = self.travel_world() {
                self.travel.note_world(&world);
            }
            // Once the travel has planned at all, a dungeon the character enters has its region
            // built off the frame, ahead of any need for it.
            let at = self
                .world
                .as_ref()
                .and_then(|w| w.character.as_ref())
                .filter(|_| self.playable_at.is_some())
                .map(crate::character::Character::position);
            self.travel.idle(at.as_ref());
            self.travel_player_moves = self.player_moves;
            self.travel_status = crate::travel::TravelStatus {
                events: self.travel.take_events(),
                trip: None,
            };
            return;
        }
        let playable = self.playable_at.is_some();
        let character = self.world.as_ref().and_then(|w| w.character.as_ref());
        let at = character
            .filter(|_| playable)
            .map(crate::character::Character::position);
        let land = character.map(|c| std::sync::Arc::clone(c.land()));
        let obstacles = character.map(|c| nav_obstacles(&self.objects.world, c));
        let portals = nav_portals(&self.objects.world, character);
        let idle = self.teleport.anim.state == crate::teleport::TeleportAnimState::Off;
        let world = self.travel_world();
        if self.nav_spells.is_none() && at.is_some() {
            self.nav_spells = self
                .store
                .read(crate::hud::SPELL_TABLE)
                .ok()
                .and_then(|b| {
                    <dereth_assets::tables::SpellTable as dereth_assets::Decode>::decode_payload(
                        crate::hud::SPELL_TABLE,
                        &b,
                    )
                    .ok()
                })
                .map(std::sync::Arc::new);
        }
        if self.nav_academies.is_none() && at.is_some() {
            self.nav_academies = Some(academy_blocks(&self.store));
        }
        let hand_backs = nav_hand_backs(
            &self.objects.world,
            character,
            self.nav_academies.as_deref().unwrap_or(&[]),
        );
        let mut chat = Vec::new();
        // Without `--nav-proof`, `--nav-prep`'s lines are typed once, five seconds into the world.
        if self.nav_proof.is_none()
            && !self.cfg.nav_prep.is_empty()
            && self
                .playable_at
                .is_some_and(|t| self.timer.local_time - t > 5.0)
        {
            chat.append(&mut self.cfg.nav_prep);
        }
        if let Some(proof) = self.nav_proof.as_mut().filter(|p| !p.done()) {
            let commands = proof.use_time(
                &crate::nav_proof::ProofInput {
                    wall: self.timer.local_time,
                    at,
                    idle,
                    land: land.as_ref(),
                    store: &self.store,
                    world: world.as_deref(),
                    events: &self.travel_status.events,
                },
                &mut self.travel,
            );
            for c in commands {
                match c {
                    crate::nav_proof::ProofCommand::Chat(text) => chat.push(text),
                    crate::nav_proof::ProofCommand::Finish => {
                        // End the `--linger` stay now: the deadline check logs off on this frame.
                        self.cfg.linger = f64::MIN_POSITIVE;
                    }
                }
            }
        }
        if let Some(list) = obstacles {
            self.travel.set_obstacles(list, at.as_ref());
        }
        // A door the server answered as locked is planned round; one only slow to open is not.
        for door in self.objects.take_lock_answers() {
            self.travel.door_answered_locked(door);
        }
        let objects = &self.objects;
        let object =
            |id: dereth_primitives::ObjectId| objects.presence(id).and_then(|p| p.position);
        let holds = |name: &str| nav_held(&objects.world, name);
        let answer = self.interaction.take_travel_answer();
        let commands = self.travel.use_time(&crate::travel::TravelInput {
            now: self.timer.cur_time,
            wall: self.timer.local_time,
            at,
            idle,
            land: land.as_ref(),
            store: &self.store,
            character: nav_character(&self.objects.world, self.nav_spells.as_deref()),
            stance: nav_stance(&self.objects.world),
            portals,
            world,
            // The player's own movement since the travel last ran, or about to be carried out this
            // frame: a key, or what an interface's stick or mouse buttons asked for.
            player_input: self.player_moves != self.travel_player_moves
                || self.actions.player_movement()
                || self.orbit_pending.iter().any(crate::actions::is_movement),
            fizzled: std::mem::take(&mut self.travel_fizzles) > 0,
            object: &object,
            hand_backs,
            holds: &holds,
            answer,
        });
        self.travel_player_moves = self.player_moves;
        self.travel_status = crate::travel::TravelStatus {
            events: self.travel.take_events(),
            trip: self.travel.view(),
        };
        for text in chat {
            tracing::info!("--nav-prep: {text:?}");
            self.submit_requests(vec![dereth_client_contract::UiRequest::ChatLine {
                text,
                window: 0,
            }]);
        }
        for c in commands {
            use crate::travel::TravelCommand;
            use dereth_client_contract::actions::movement::{MOVE_FORWARD, TURN_LEFT, TURN_RIGHT};
            use dereth_client_contract::UiRequest;
            match c {
                TravelCommand::Forward(on) => self.actions.steer(if on {
                    crate::actions::Action::begin(MOVE_FORWARD)
                } else {
                    crate::actions::Action::end(MOVE_FORWARD)
                }),
                TravelCommand::Turn(turn) => {
                    // Release both, then hold the one asked for.
                    self.actions.steer(crate::actions::Action::end(TURN_LEFT));
                    self.actions.steer(crate::actions::Action::end(TURN_RIGHT));
                    if let Some(right) = turn {
                        let id = if right { TURN_RIGHT } else { TURN_LEFT };
                        self.actions.steer(crate::actions::Action::begin(id));
                    }
                }
                TravelCommand::Use(id) => self.submit_requests(vec![UiRequest::Use(id)]),
                TravelCommand::Wield(item) => self.submit_requests(vec![UiRequest::Inventory(
                    dereth_client_contract::view::InventoryIntent::Wield { item },
                )]),
                TravelCommand::ToggleCombat => {
                    self.submit_requests(vec![UiRequest::ToggleCombatMode]);
                }
                TravelCommand::Cast(spell_id) => {
                    // As a cast does on its first call: the spell table into the magic system
                    // when it has none, or the cast has no spell to find and does nothing.
                    if self.objects.world.magic.spell_table.is_none() {
                        self.objects
                            .world
                            .magic
                            .spell_table
                            .clone_from(&self.nav_spells);
                    }
                    self.submit_requests(vec![UiRequest::CastSpell { spell_id }]);
                }
                TravelCommand::Chat(text) => {
                    // A journey's own line, which the player did not type.
                    self.submit_requests(vec![UiRequest::ChatLineOnBehalf { text, window: 0 }]);
                }
                TravelCommand::Give { item, target } => {
                    self.submit_requests(vec![UiRequest::Inventory(
                        dereth_client_contract::view::InventoryIntent::Give {
                            item,
                            target,
                            amount: 1,
                        },
                    )]);
                }
                TravelCommand::Ask(question) => self.interaction.ask_travel(question),
            }
        }
    }

    /// Capture a screenshot using the original filename rule, verified against retail.
    ///
    /// So: **the directory of the preferences file** — the default preferences file is the same
    /// global the device input takes the `.keymap` directory from, which is why
    /// the screenshot lands beside `acclient.keymap` and not beside the DATs — then `ScreenShot` +
    /// a **five-digit zero-padded** index, the **lowest index whose file does not already exist**,
    /// capped at `99999`: the loop stops at `99999` when every name is taken, and saves over that
    /// one. An empty default preferences file gives an empty directory and therefore a path
    /// relative to the process working directory, which is retail's behaviour and is reproduced
    /// rather than special-cased.
    ///
    /// Screenshots are encoded as PNG. This workspace has
    /// **no JPEG encoder** — `zune-jpeg`
    /// in `dereth-render` is a decoder, and adding one would change `Cargo.lock`, which `--locked`
    /// forbids — so the file is a PNG through the existing `Gpu::capture_png` read-back and is
    /// named `.png`. Naming a PNG `.jpg` would be worse than the honest extension; the numbering,
    /// the padding and the directory are retail's, and swapping the encoder later changes one
    /// literal here and one in `capture_png`'s caller.
    fn screenshot_path(&self) -> std::path::PathBuf {
        // Resolve the screenshots directory. `Path::parent` of an
        // empty path is `Some("")`, which is the empty-`%s` case above.
        let dir = self
            .cfg
            .preferences_file
            .parent()
            .map_or_else(std::path::PathBuf::new, std::path::Path::to_path_buf);
        let name = |n: u32| dir.join(format!("ScreenShot{n:05}.png"));
        // Retail checks `_access(name, 0)` — "does it exist". First miss wins.
        (0..100_000_u32)
            .map(name)
            .find(|p| !p.exists())
            .unwrap_or_else(|| name(99_999))
    }

    /// `--capture-at` and `--set-at`, at the top of a frame. A picture is taken once its frame has
    /// been drawn, so it is the last presented frame; a setting is made as its frame starts, the
    /// way an options page's change is, and its owner applies it in this frame's drains.
    pub(super) fn scripted_use_time(&mut self) {
        self.frames_begun += 1;
        if self.cfg.capture_at.is_empty()
            && self.cfg.set_at.is_empty()
            && self.cfg.action_at.is_empty()
        {
            return;
        }
        // Frames are counted from one, as `--frames` counts them; the one before this is the one
        // just drawn.
        let drawn = self.frames_begun - 1;
        let pictures: Vec<std::path::PathBuf> = self
            .cfg
            .capture_at
            .iter()
            .filter(|(f, _)| *f == drawn)
            .map(|(_, p)| p.clone())
            .collect();
        for path in pictures {
            match self.present.capture_png(&path) {
                Ok(()) => tracing::info!("frame {drawn} captured to {}", path.display()),
                Err(e) => tracing::warn!("--capture-at {drawn}: {e}"),
            }
        }
        let actions: Vec<u32> = self
            .cfg
            .action_at
            .iter()
            .filter(|(f, _)| *f == drawn + 1)
            .map(|(_, a)| *a)
            .collect();
        for action in actions {
            tracing::info!("frame {}: action {action:#X}", drawn + 1);
            self.scripted_actions
                .push(dereth_client_contract::actions::ActionId(action));
        }
        let settings: Vec<String> = self
            .cfg
            .set_at
            .iter()
            .filter(|(f, _)| *f == drawn + 1)
            .map(|(_, s)| s.clone())
            .collect();
        for setting in settings {
            match scripted_preference(&setting) {
                Some((name, value)) => {
                    tracing::info!("frame {}: {name} = {value:?}", drawn + 1);
                    let _ = dereth_client_contract::options::store::set_value(name, value.clone());
                    self.scripted_preferences.push((name, value));
                }
                None => {
                    tracing::warn!("--set-at {setting:?} names no preference this client sets");
                }
            }
        }
    }

    /// The landscape styles the scene refused this poll: the options value goes back to the style
    /// still drawn, so a page shows what is on screen, and the player is told why in the chat
    /// window, on the channel the client's own refusals use.
    pub(super) fn report_landscape_refusals(&mut self, w: &crate::frame_events::RenderPrefWork) {
        use dereth_client_contract::options::landscape::{Landscape, RegionStyle, WORLD_DEFAULT};
        #[cfg(feature = "hifi")]
        if w.fidelity_refused {
            // Why, as the device and the renderer chosen for the next start say it: another
            // renderer, or a restart that asks for the presentation. Each line once a session.
            let why = self.hifi_availability().refusal();
            tracing::warn!("{why}");
            if !self.fidelity_told.contains(&why) {
                self.fidelity_told.push(why);
                self.objects.world.scroll.add_feedback_to_scroll(
                    why,
                    dereth_client_model::scroll::LOCAL_ERROR_TYPE,
                    true,
                    0,
                    dereth_client_contract::feedback::Feedback::LOCAL,
                );
            }
        }
        for (which, refused) in [
            (Landscape::Ground, w.ground_refused),
            (Landscape::Sky, w.sky_refused),
            (Landscape::Objects, w.objects_refused),
        ] {
            let Some((files, kept)) = refused else {
                continue;
            };
            let kept_value = dereth_client_contract::PrefValue::Int(
                kept.map_or(WORLD_DEFAULT, RegionStyle::value),
            );
            let _ =
                dereth_client_contract::options::store::set_value(which.name(), kept_value.clone());
            // The next login's scene keeps what is drawn too.
            for scene in [self.pending_scene.as_mut(), self.scene_config.as_mut()]
                .into_iter()
                .flatten()
            {
                scene.render.set_named(which.name(), &kept_value);
            }
            let text = which.notice(files);
            tracing::warn!("{text}");
            self.objects.world.scroll.add_feedback_to_scroll(
                text,
                dereth_client_model::scroll::LOCAL_ERROR_TYPE,
                true,
                0,
                dereth_client_contract::feedback::Feedback::LOCAL,
            );
        }
    }

    /// Capture the screenshot through the presentation, the half that requires the device.
    ///
    /// On success the client formats the chosen path into its confirmation line, so the player
    /// sees the saved filename. Channel `0x1A` is
    /// `dereth_client_model::scroll::LOCAL_ERROR_TYPE`, which is the channel every
    /// combat and client-UI scroll-text calls use.
    ///
    /// The path is the client's directory and numbering, not the process temp directory and a
    /// count of screenshots already taken; see [`Self::screenshot_path`] for the rule and the one
    /// deviation that is left.
    ///
    /// **The remaining deviation on this side:** retail captures from inside the action while this
    /// reads back the buffer the device holds when the drain runs, which is the previously
    /// presented frame.
    pub(super) fn take_action_screenshot(&mut self) {
        let path = self.screenshot_path();
        match self.present.capture_png(&path) {
            Ok(()) => {
                self.events.push(FrameEvent::ScreenshotSaved);
                let text = format!("Screenshot saved to file '{}'", path.display());
                self.objects.world.scroll.add_feedback_to_scroll(
                    &text,
                    dereth_client_model::scroll::LOCAL_ERROR_TYPE,
                    true,
                    0,
                    dereth_client_contract::feedback::Feedback::LOCAL,
                );
            }
            Err(e) => {
                // The arm still returns TRUE and still prints nothing, so the only place this can
                // be seen is here.
                self.events.push(FrameEvent::ScreenshotFailed);
                tracing::warn!("screenshot refused: {e}");
            }
        }
    }
}
