//! `--explore` carried out: the free camera's controls, its speed, and the location it copies.
//! See [`crate::explore`].

use super::*;

use crate::explore::{Fly, Status};

impl<S: Shell> App<S> {
    /// Whether the client runs in explore mode.
    #[must_use]
    pub fn exploring(&self) -> bool {
        self.explore.is_some()
    }

    /// One of the camera's ways held or let go, while the mode runs.
    pub fn explore_fly(&mut self, way: Fly, held: bool) {
        if self.explore.is_some() {
            way.hold(&mut self.input, held);
        }
    }

    /// The camera `steps` speeds faster, or slower below zero.
    pub fn explore_speed(&mut self, steps: i32) {
        if let Some(explore) = self.explore.as_mut() {
            explore.change_speed(steps);
        }
    }

    /// What the screen says while the mode runs; `None` outside it.
    #[must_use]
    pub fn explore_status(&self) -> Option<Status> {
        let explore = self.explore?;
        let location = self.world.as_ref().and_then(|ws| {
            ws.viewer_block()
                .map(|block| crate::explore::short_location(&ws.camera, block))
        });
        Some(Status {
            meters_per_second: explore.meters_per_second(),
            location,
        })
    }

    /// The camera's spot as `@loc` says a place, `Your location is: …`; `None` before the world
    /// is up.
    #[must_use]
    pub fn camera_location_line(&self) -> Option<String> {
        let ws = self.world.as_ref()?;
        let block = ws.viewer_block()?;
        Some(crate::explore::location_line(&ws.camera, block))
    }
}
