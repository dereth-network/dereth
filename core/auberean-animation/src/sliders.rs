//! A body's sliders: named values the animation graph reads (forward speed, sideways speed,
//! turning, the random picks of idles...), each moving toward the value it was last asked for at a
//! steady rate, and the crossfades between numbered choices that random nodes run on a slider.
//!
//! The tracker works only while something is moving: a frame in which no slider moved and no
//! crossfade ran leaves it idle, and an idle tracker does not advance (not even the time since a
//! crossfade's end) until a slider is next asked to move over time.

use crate::table::Table;

/// The sliders a body starts at 1.
pub const STARTING_SLIDERS: [u32; 6] = [0x4000_000A, OPACITY, RED, GREEN, BLUE, ALPHA];

/// The sliders an object's draws take their color from.
const OPACITY: u32 = 0x4000_0015;
const RED: u32 = 0x4000_0016;
const GREEN: u32 = 0x4000_0017;
const BLUE: u32 = 0x4000_0018;
const ALPHA: u32 = 0x4000_001A;

/// A change smaller than this is none.
const EPSILON: f32 = 0.0002;

/// One slider: its value, its rate toward the value asked for, and who asked.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SliderState {
    pub value: f32,
    pub velocity: f32,
    pub desired: f32,
    pub context: u32,
}

/// A crossfade on a slider: from one choice to another over a length of time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderInterp {
    /// The time since it started (or, once over, since it ended).
    pub time: f32,
    pub active: bool,
    pub start: f32,
    pub end: f32,
    /// How far through, 0 to 1.
    pub current: f32,
    pub length: f32,
    pub context: u32,
}

impl Default for SliderInterp {
    // D2-06509
    fn default() -> Self {
        Self {
            time: 0.0,
            active: false,
            start: 0.0,
            end: 0.0,
            current: 0.0,
            length: 1.0,
            context: 0,
        }
    }
}

impl SliderInterp {
    /// Start a crossfade from `start` to `end` over `length` seconds.
    // D2-15179
    pub fn activate(&mut self, start: f32, end: f32, length: f32) {
        self.start = start;
        self.active = true;
        self.end = end;
        self.time = 0.0;
        self.length = length;
    }
}

/// Someone asked to hear when a slider move was taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SliderDone {
    pub context: u32,
}

/// A body's sliders and crossfades.
#[derive(Clone, Debug, PartialEq)]
pub struct Sliders {
    pub infos: Table<SliderState>,
    pub interps: Table<SliderInterp>,
    /// Something moved last frame, or a move was asked for since.
    pub active: bool,
    /// The moves whose askers are to hear (taken by the body's owner).
    pub done: Vec<SliderDone>,
}

impl Default for Sliders {
    // D2-15207, D2-15200, D2-15183, D2-15184, D2-15187, D2-15188, D2-15203
    fn default() -> Self {
        Self {
            infos: Table::default(),
            interps: Table::default(),
            active: true,
            done: Vec::new(),
        }
    }
}

impl Sliders {
    /// A tracker with the starting sliders at 1.
    // D2-15204
    #[must_use]
    pub fn new() -> Self {
        let mut t = Self::default();
        for s in STARTING_SLIDERS {
            t.do_slider(0, s, 1.0, 0.0);
        }
        t
    }

    /// Ask slider `slider` to move to `value` over `time` seconds (at once when under 0.0002). A
    /// slider already headed within 0.0002 of the value is left alone. Whoever asked (`context`) is
    /// told at once either way.
    // D2-15201
    pub fn do_slider(&mut self, context: u32, slider: u32, value: f32, time: f32) -> bool {
        let desired = self.infos.get(slider).map_or(0.0, |i| i.desired);
        if (value - desired).abs() < EPSILON {
            if context != 0 {
                self.done.push(SliderDone { context });
            }
            return true;
        }
        if time != 0.0 {
            self.active = true;
        }
        let info = self.infos.entry_or(
            slider,
            SliderState {
                context,
                ..SliderState::default()
            },
        );
        if context != 0 {
            self.done.push(SliderDone { context });
        }
        info.desired = value;
        if time < EPSILON {
            info.value = value;
            info.velocity = 0.0;
            return true;
        }
        info.velocity = (value - info.value) / time;
        true
    }

    /// A slider's value now; 0 for a slider never set.
    // D2-15189, D2-05897, D2-06393
    #[must_use]
    pub fn current(&self, slider: u32) -> f32 {
        self.infos.get(slider).map_or(0.0, |i| i.value)
    }

    /// The object's opacity, held to 0..1.
    // D2-06524
    #[must_use]
    pub fn opacity(&self) -> f32 {
        self.current(OPACITY).clamp(0.0, 1.0)
    }

    /// What an object's draws are multiplied by, as these sliders hold it: red, green, blue and
    /// alpha, each held to 0..1, the alpha times the opacity (its own slider, held to 0..1).
    // D2-06487, D2-06380
    #[must_use]
    pub fn draw_color(&self) -> [f32; 4] {
        let held = |slider| self.current(slider).clamp(0.0, 1.0);
        [
            held(RED),
            held(GREEN),
            held(BLUE),
            held(ALPHA) * held(OPACITY),
        ]
    }

    /// The value a slider is headed for.
    // D2-15190, D2-05898, D2-06394
    #[must_use]
    pub fn desired(&self, slider: u32) -> f32 {
        self.infos.get(slider).map_or(0.0, |i| i.desired)
    }

    /// How fast a slider moves, a second.
    // D2-15191
    #[must_use]
    pub fn velocity(&self, slider: u32) -> f32 {
        self.infos.get(slider).map_or(0.0, |i| i.velocity)
    }

    /// A slider's crossfade, made (idle, a second long) when it has none.
    // D2-15202
    pub fn interp(&mut self, slider: u32) -> &mut SliderInterp {
        self.interps.entry_or(slider, SliderInterp::default())
    }

    /// The sliders the network sets, in the tracker's order, by the template's controllers.
    // D2-15186
    pub fn network_sliders(&self, controller: impl Fn(u32) -> Option<u32>) -> Vec<u32> {
        self.infos
            .keys()
            .filter(|&s| controller(s) == Some(crate::queries::NETWORK_CONTROLLED))
            .collect()
    }

    /// Advance `dt` seconds, when active. Each moving slider steps by its rate and stops on the
    /// value asked for once it reaches or passes it; each crossfade's time grows, and a running one
    /// that has gone past its length ends on its choice (its start becomes its end) and starts
    /// counting again from 0. The tracker stays active only while a slider moved or a crossfade
    /// ran.
    // D2-15185
    pub fn update(&mut self, dt: f32) {
        if !self.active {
            return;
        }
        self.active = false;
        for (_, i) in self.infos.iter_mut() {
            if i.velocity.abs() < EPSILON {
                continue;
            }
            self.active = true;
            let v = dt * i.velocity + i.value;
            if (i.velocity < 0.0 && v <= i.desired) || (i.velocity > 0.0 && i.desired <= v) {
                i.value = i.desired;
                i.velocity = 0.0;
            } else {
                i.value = v;
            }
        }
        for (_, c) in self.interps.iter_mut() {
            c.time += dt;
            if c.active {
                self.active = true;
                if c.length < c.time {
                    c.active = false;
                    c.time = 0.0;
                    c.start = c.end;
                    c.end = 0.0;
                }
                c.current = c.time / c.length;
            }
        }
    }

    /// Set a slider at once (no asker).
    pub fn set(&mut self, slider: u32, value: f32) {
        self.do_slider(0, slider, value, 0.0);
    }

    /// A slider's value now.
    #[must_use]
    pub fn get(&self, slider: u32) -> f32 {
        self.current(slider)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (unit checks of the slider arithmetic against its definition)
    use super::*;

    #[test]
    fn a_slider_moves_at_its_rate_and_stops_on_its_value() {
        let mut t = Sliders::new();
        assert_eq!(t.current(0x4000_0015), 1.0, "the starting sliders");
        t.do_slider(0, 7, 2.0, 0.5);
        assert_eq!(t.velocity(7), 4.0);
        t.update(0.25);
        assert!((t.current(7) - 1.0).abs() < 1e-6);
        t.update(0.5);
        assert_eq!(t.current(7), 2.0, "stops on the value");
        assert!(t.active, "it moved this frame");
        t.update(0.1);
        assert!(!t.active, "nothing moved: idle");
        let c = t.interp(9);
        c.activate(0.0, 2.0, 0.2);
        t.update(0.1);
        assert!(!t.active, "an idle tracker does not run its crossfades");
        t.do_slider(0, 7, 3.0, 1.0);
        t.update(0.1);
        assert!((t.interp(9).current - 0.5).abs() < 1e-6);
        t.update(0.15);
        let c = *t.interp(9);
        assert!(!c.active && c.start == 2.0 && c.time == 0.0, "{c:?}");
    }
}
