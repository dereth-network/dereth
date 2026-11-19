//! Explore mode (`--explore`): the world with no server and no body, and a free camera flown
//! through it at anything from a walk to thousands of meters a second
//! ([`auberean_scene::explore`]). No interface is shown and none takes the keys; a line in the
//! corner says how fast the camera flies, where it is and which keys do what.
//!
//! The right button held turns the camera with the mouse; W, A, S and D (or the arrows) fly it, E
//! up and Q down; the wheel, or + and -, step its speed; C copies where it is as a location line,
//! which `--explore` reads back.

use auberean_scene::explore::{FreeCamera, Ways, SPEEDS, START_SPEED};

use crate::input::{Button, Event, Key};
use crate::{Host, Output, Shell};

/// Degrees the camera turns for each count of the mouse's motion.
const TURN_PER_COUNT: f32 = 0.15;

/// How long the line saying what was copied stays, seconds.
const NOTE_SECONDS: f64 = 3.0;

/// What the keys do, as the corner line says it.
pub const KEYS: &str =
    "Right mouse: look   W A S D: fly   E, Q: up, down   Wheel or + -: speed   C: copy location";

/// A way the camera flies while its key is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    Forward,
    Back,
    Left,
    Right,
    Up,
    Down,
}

/// What a key does in explore mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExploreKey {
    Fly(Way),
    Faster,
    Slower,
    CopyLocation,
}

/// The key `key` does, if any.
#[must_use]
pub const fn key_of(key: Key) -> Option<ExploreKey> {
    Some(match key {
        Key::W | Key::ArrowUp => ExploreKey::Fly(Way::Forward),
        Key::S | Key::ArrowDown => ExploreKey::Fly(Way::Back),
        Key::A | Key::ArrowLeft => ExploreKey::Fly(Way::Left),
        Key::D | Key::ArrowRight => ExploreKey::Fly(Way::Right),
        Key::E => ExploreKey::Fly(Way::Up),
        Key::Q => ExploreKey::Fly(Way::Down),
        Key::Equal | Key::NumpadAdd => ExploreKey::Faster,
        Key::Minus | Key::NumpadSubtract => ExploreKey::Slower,
        Key::C => ExploreKey::CopyLocation,
        _ => return None,
    })
}

/// The mode's state while it runs.
#[derive(Clone, Debug)]
pub struct Explore {
    /// Where the camera is and which way it looks.
    pub camera: FreeCamera,
    /// An index into [`SPEEDS`].
    speed: usize,
    /// The ways held.
    ways: Ways,
    /// The right button is held: the mouse turns the camera.
    looking: bool,
    /// What was last copied, and when.
    note: Option<(&'static str, f64)>,
    /// Wheel notches not yet taken as a step of speed.
    wheel: f32,
}

impl Explore {
    /// The mode with the camera at `camera`, at the starting speed.
    #[must_use]
    pub const fn new(camera: FreeCamera) -> Self {
        Self {
            camera,
            speed: START_SPEED,
            ways: Ways {
                forward: false,
                back: false,
                left: false,
                right: false,
                up: false,
                down: false,
            },
            looking: false,
            note: None,
            wheel: 0.0,
        }
    }

    /// The flying speed, meters a second.
    #[must_use]
    pub fn meters_per_second(&self) -> f32 {
        SPEEDS[self.speed.min(SPEEDS.len() - 1)]
    }

    /// `steps` speeds faster (or slower, below zero), held between the slowest and the fastest.
    pub fn change_speed(&mut self, steps: i32) {
        let last = i64::try_from(SPEEDS.len() - 1).unwrap_or(0);
        let at = i64::try_from(self.speed).unwrap_or(0) + i64::from(steps);
        self.speed = usize::try_from(at.clamp(0, last)).unwrap_or(0);
    }

    fn hold(&mut self, way: Way, held: bool) {
        let w = &mut self.ways;
        let slot = match way {
            Way::Forward => &mut w.forward,
            Way::Back => &mut w.back,
            Way::Left => &mut w.left,
            Way::Right => &mut w.right,
            Way::Up => &mut w.up,
            Way::Down => &mut w.down,
        };
        *slot = held;
    }

    /// The corner's lines at `now`: the speed and the camera's spot, the keys, and what was copied
    /// while that is new.
    #[must_use]
    pub fn lines(&self, now: f64) -> Vec<String> {
        let mut lines = vec![
            format!(
                "Explore   {}   {}",
                speed_text(self.meters_per_second()),
                self.camera.short_location()
            ),
            KEYS.to_owned(),
        ];
        if let Some((note, at)) = self.note {
            if now - at < NOTE_SECONDS {
                lines.push(note.to_owned());
            }
        }
        lines
    }
}

/// A speed as the corner line says it.
#[must_use]
pub fn speed_text(meters_per_second: f32) -> String {
    format!("{meters_per_second:.0} m/s")
}

impl<H: Host> Shell<'_, H> {
    /// Run in explore mode from `camera`: no interface and no session, the camera flown by the keys.
    pub fn start_explore(&mut self, camera: FreeCamera) {
        self.explore = Some(Explore::new(camera));
        self.host.set_title("Auberean | Explore");
    }

    /// Whether the client runs in explore mode.
    #[must_use]
    pub const fn exploring(&self) -> bool {
        self.explore.is_some()
    }

    /// One event in explore mode: the window's own, and the camera's keys and mouse. None reaches
    /// the game's interface or its keys.
    pub(crate) fn explore_event(&mut self, e: &Event) {
        let now = self.host.now();
        let Some(x) = self.explore.as_mut() else {
            return;
        };
        match e {
            Event::Resized { width, height } => {
                self.size = ((*width).max(1), (*height).max(1));
                if !self.settings.full_screen {
                    self.settings.window = self.size;
                }
            }
            Event::Close => self.quit = true,
            // Away from the window nothing stays held.
            Event::Focus(false) => {
                x.ways = Ways::default();
                if x.looking {
                    x.looking = false;
                    self.host.hold_pointer(false);
                }
            }
            Event::Key { key, down, repeat } => match key_of(*key) {
                Some(ExploreKey::Fly(way)) => x.hold(way, *down),
                Some(ExploreKey::Faster) if *down => x.change_speed(1),
                Some(ExploreKey::Slower) if *down => x.change_speed(-1),
                Some(ExploreKey::CopyLocation) if *down && !*repeat => {
                    let line = x.camera.location_line();
                    tracing::info!("{line}");
                    self.host.set_clipboard_text(&line);
                    x.note = Some(("Location copied.", now));
                }
                _ => {}
            },
            Event::Button {
                button: Button::Right,
                down,
            } => {
                x.looking = *down;
                self.host.hold_pointer(*down);
            }
            Event::Motion { dx, dy } if x.looking => {
                x.camera.turn(dx * TURN_PER_COUNT, -dy * TURN_PER_COUNT);
            }
            Event::Wheel { notches } => {
                x.wheel += notches;
                // LINT-OK: whole notches of a wheel, a handful at most.
                #[allow(clippy::cast_possible_truncation)]
                let steps = x.wheel.trunc() as i32;
                x.wheel -= x.wheel.trunc();
                if steps != 0 {
                    x.change_speed(steps);
                }
            }
            _ => {}
        }
    }

    /// One frame in explore mode: the camera flown by the ways held for the time since the last,
    /// held over the ground, the world drawn from it and the corner's lines over it.
    pub(crate) fn explore_frame(&mut self) -> Output<'_> {
        let now = self.host.now();
        // LINT-OK: a frame's seconds, a quarter at most.
        #[allow(clippy::cast_possible_truncation)]
        let dt = (now - self.last).clamp(0.0, 0.25) as f32;
        self.last = now;
        self.frames.push_back(now);
        while self.frames.front().is_some_and(|t| now - t > 1.0) {
            self.frames.pop_front();
        }
        let t = self.host.now();
        if let Some(x) = self.explore.as_mut() {
            let meters = x.meters_per_second() * dt;
            x.camera.fly(x.ways, meters);
            self.scene.hold_over_ground(&mut x.camera);
            let camera = x.camera;
            self.scene.build_explore(now, &camera);
            // The ground first drawn under a camera that was looking for it.
            self.scene.hold_over_ground(&mut x.camera);
            if x.camera != camera {
                let camera = x.camera;
                self.scene.build_explore(now, &camera);
            }
        }
        self.costs[2] = self.host.now() - t;
        let mut overlay = std::mem::take(&mut self.overlay);
        overlay.quads.clear();
        overlay.items.clear();
        overlay.scenes.clear();
        if let Some(x) = &self.explore {
            crate::dev::overlay(&mut overlay, &x.lines(now));
        }
        self.overlay = overlay;
        let quit = self.quit;
        let frames = self.scene.frames(now, true);
        Output {
            world: frames.world,
            portal: None,
            portal_alpha: 0.0,
            veil: None,
            overlay: &self.overlay,
            views: Vec::new(),
            fade: Vec::new(),
            warm: frames.warm,
            quit,
            screenshot: false,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a developer's camera flown through the world with no server)
    use super::*;

    #[test]
    fn the_keys_fly_the_camera_step_its_speed_and_copy() {
        assert_eq!(key_of(Key::W), Some(ExploreKey::Fly(Way::Forward)));
        assert_eq!(key_of(Key::ArrowUp), Some(ExploreKey::Fly(Way::Forward)));
        assert_eq!(key_of(Key::S), Some(ExploreKey::Fly(Way::Back)));
        assert_eq!(key_of(Key::A), Some(ExploreKey::Fly(Way::Left)));
        assert_eq!(key_of(Key::ArrowRight), Some(ExploreKey::Fly(Way::Right)));
        assert_eq!(key_of(Key::E), Some(ExploreKey::Fly(Way::Up)));
        assert_eq!(key_of(Key::Q), Some(ExploreKey::Fly(Way::Down)));
        assert_eq!(key_of(Key::Equal), Some(ExploreKey::Faster));
        assert_eq!(key_of(Key::NumpadSubtract), Some(ExploreKey::Slower));
        assert_eq!(key_of(Key::C), Some(ExploreKey::CopyLocation));
        assert_eq!(key_of(Key::Enter), None);
    }

    #[test]
    fn the_speed_steps_between_a_walk_and_thousands_of_meters_a_second() {
        let camera = auberean_scene::explore::parse_start(auberean_scene::explore::DEFAULT_START)
            .expect("the default start reads");
        let mut e = Explore::new(camera);
        assert_eq!(speed_text(e.meters_per_second()), "100 m/s");
        e.change_speed(100);
        assert_eq!(speed_text(e.meters_per_second()), "5000 m/s");
        e.change_speed(-1);
        assert_eq!(speed_text(e.meters_per_second()), "2500 m/s");
        e.change_speed(-100);
        assert_eq!(speed_text(e.meters_per_second()), "2 m/s");
    }
}
