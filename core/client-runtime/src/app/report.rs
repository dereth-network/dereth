//! `@report` and `@perfcapture` carried out (CD-report-commands): the report assembled from the
//! world, the camera and the presentation, and the capture recorded frame by frame and saved.

use super::*;

use crate::perf_capture::PerfCapture;
use crate::report::{ReportCamera, ReportTime, VisualReport};

impl<S: Shell> App<S> {
    /// What `@report` says about the frame last drawn: where the player stands, where the camera
    /// is and how the frame is drawn. See [`crate::report`].
    #[must_use]
    pub fn visual_report(&self, shell: &S) -> VisualReport {
        let ws = self.world.as_ref();
        let character = ws.and_then(|w| w.character.as_ref());
        let mut r = VisualReport {
            loc: character.map(crate::character::Character::position),
            ..VisualReport::default()
        };
        if let Some(ws) = ws {
            if let Some(block) = ws.viewer_block() {
                let p = ws.camera.position;
                // The cell the camera stands in: the chase camera's own where it has one, else
                // the outdoor cell under the camera.
                let cell = character
                    .and_then(|c| c.camera.viewer_cell)
                    .map(|c| c.0)
                    .unwrap_or_else(|| {
                        let (bx, by) = ((p.x / 192.0).floor(), (p.y / 192.0).floor());
                        // LINT-OK: a camera a block or two from the block the world is drawn
                        // around.
                        #[allow(clippy::cast_possible_truncation)]
                        let here =
                            crate::report::block_id((block.0 + bx as i32, block.1 + by as i32));
                        crate::report::outdoor_cell(here, p.x - bx * 192.0, p.y - by * 192.0)
                    });
                r.camera = Some(ReportCamera::from_render(&ws.camera, block, Some(cell)));
            }
            r.time = Some(ReportTime {
                fraction: ws.clock.present_time_of_day,
                name: ws.clock.time_of_day_name().unwrap_or_default().to_owned(),
                set_here: ws.clock.is_adjusted(),
                clock: self.timer.cur_time + ws.clock.time_zero_start_delta,
            });
            r.weather = ws.weather.word().to_owned();
        }
        r.weather_off = self
            .objects
            .world
            .player_system
            .options
            .get(dereth_client_model::player::option::DISABLE_MOST_WEATHER_EFFECTS);
        r.mode = match character {
            None => "free",
            Some(c) if c.camera.orbit_active => "orbit",
            Some(c) if c.camera.set.in_map_mode => "map",
            Some(c) if crate::camera::CameraState::in_head(&c.camera.manager) => "first person",
            Some(_) => "chase",
        }
        .to_owned();
        let (width, height) = self.present.size();
        r.window = (width, height);
        let viewport = shell.game_viewport();
        r.viewport =
            Some(viewport.map_or((0, 0, width, height), |v| (v.x, v.y, v.width, v.height)));
        if let Some(scene) = self.present.scene(ws) {
            let prefs = scene.render_preferences();
            r.fov = prefs.field_of_view;
            r.aspect = prefs.aspect_ratio;
            r.draw_distance = prefs.landscape_draw_distance;
            r.vertical_fov = scene.view_params(width, height).fov_y_rad.to_degrees();
        }
        r.render = self.present.render_context(ws);
        // The eye, where it is drawn from somewhere other than the camera.
        if let (Some(eye), Some(ws)) = (r.render.drawn_eye, ws) {
            if eye != ws.camera.position {
                r.eye = Some(eye);
            }
        }
        r.build = format!("{} {}", self.interaction.client_build_id, self.build_commit);
        r
    }

    /// Carry out the `@report` and `@perfcapture` commands typed this frame, answering each in
    /// the chat window it was typed in. Nothing is sent.
    pub(super) fn report_use_time(&mut self, shell: &mut S) {
        use dereth_client_model::chat::text_type;
        use dereth_client_model::cmd::report as words;
        let say = |app: &mut Self, text: &str, ty: u32, window: u32| {
            app.objects.world.scroll.add_feedback_to_scroll(
                text,
                ty,
                true,
                window,
                dereth_client_contract::feedback::Feedback::LOCAL,
            );
        };
        for window in self.interaction.take_report_requests() {
            if self.world.is_none() {
                say(self, words::NO_WORLD, text_type::LOCAL_ERROR, window);
                continue;
            }
            let line = self.visual_report(shell).line();
            tracing::info!("{line}");
            say(self, &line, text_type::DEFAULT, window);
            if shell.copy_text(&line) {
                say(self, words::COPIED, text_type::DEFAULT, window);
            }
        }
        for (seconds, window) in self.interaction.take_perf_capture_requests() {
            if self.perf_capture.is_some() {
                say(
                    self,
                    words::ALREADY_RECORDING,
                    text_type::LOCAL_ERROR,
                    window,
                );
                continue;
            }
            let report = self.visual_report(shell).line();
            self.perf_capture = Some(PerfCapture::new(seconds, window, report));
            say(
                self,
                &words::capture_started(seconds),
                text_type::DEFAULT,
                window,
            );
        }
    }

    /// At the start of a frame, while a capture records: the frame before, `interval` seconds
    /// long, is recorded, and a capture that has recorded its seconds is saved beside the
    /// preferences file and says where.
    pub(super) fn perf_capture_use_time(&mut self, interval: Option<f32>) {
        let Some(capture) = self.perf_capture.as_mut() else {
            return;
        };
        let steps = self.perf_steps;
        let present = &self.present;
        let done = capture.frame(self.timer.elapsed_time(), |c| {
            if let Some(dt) = interval {
                c.record(dt, &steps, present.frame_costs().as_ref());
            }
        });
        if !done {
            return;
        }
        let Some(capture) = self.perf_capture.take() else {
            return;
        };
        let render = self.present.render_context(self.world.as_ref());
        let text = capture.file_text(&[
            ("gpu", render.adapter.clone()),
            ("driver", render.driver.clone()),
            ("present mode", render.present_mode.clone()),
        ]);
        let unix = self
            .clock
            .unix_time()
            .and_then(|d| i64::try_from(d.as_secs()).ok())
            .unwrap_or(0);
        let name = crate::perf_capture::file_name(unix, self.clock.utc_offset_secs(unix));
        let folder = self
            .cfg
            .preferences_file
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        let path = folder.join(name);
        let written = std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(&path, text));
        let (answer, ty) = match written {
            Ok(()) => {
                let summary = capture.summary().sentence();
                tracing::info!("performance capture saved: {} ({summary})", path.display());
                (
                    dereth_client_model::cmd::report::capture_saved(
                        &path.display().to_string(),
                        &summary,
                    ),
                    dereth_client_model::chat::text_type::DEFAULT,
                )
            }
            Err(e) => (
                dereth_client_model::cmd::report::capture_failed(&e.to_string()),
                dereth_client_model::chat::text_type::LOCAL_ERROR,
            ),
        };
        self.objects.world.scroll.add_feedback_to_scroll(
            &answer,
            ty,
            true,
            capture.window,
            dereth_client_contract::feedback::Feedback::LOCAL,
        );
    }
}
