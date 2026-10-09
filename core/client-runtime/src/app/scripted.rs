//! Scripted settings, screenshots, and render feedback.

use super::*;

impl<S: Shell> App<S> {
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
            tracing::warn!("{FIDELITY_REFUSED}");
            self.objects.world.scroll.add_feedback_to_scroll(
                FIDELITY_REFUSED,
                dereth_client_model::scroll::LOCAL_ERROR_TYPE,
                true,
                0,
                dereth_client_contract::feedback::Feedback::LOCAL,
            );
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
