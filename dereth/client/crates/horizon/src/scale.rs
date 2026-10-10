//! The interface scale: which of its steps a window offers, and the step the interface is drawn
//! at.
//!
//! Every layout is drawn for a canvas 1080 lines tall, a layout unit to a pixel at 100%; a window
//! shorter than that shrinks the layouts to fit it ([`crate::ui::HorizonUi::layout_scale`]). A
//! larger step draws everything larger and so leaves the layouts a smaller canvas, and a step is
//! offered only on a window that, at that step, still holds nine tenths of [`CANVAS`]: 150% from
//! 2160 by 1296 pixels, 200% from 2880 by 1728, 300% from 4320 by 2592. 100% is offered on every
//! window. So a 2560 by 1440 monitor offers 150%, maximised as well as full screen, and a 3840 by
//! 2160 one 200%; 1920 by 1080 offers 100% alone.
//!
//! The window is measured in its own pixels, as the shell hands the interface its size. The client
//! draws in real pixels whatever the system's display scaling, so the scaling is already in that
//! size: a 3840 by 2160 monitor set to 150% is 3840 by 2160 here, and so is a window the system
//! sizes as 1920 by 1080 of its units at 200%.
//!
//! A player who has not chosen a step is drawn at the largest the window offers, up to
//! [`AUTOMATIC_LARGEST`]. A step the player chose is drawn at while the window offers it, and the
//! automatic step while it does not, the choice kept for when it does again. Both follow the window
//! from frame to frame, as it is resized or moved to another monitor.

/// The steps, in order.
pub const STEPS: [f32; 4] = [1.0, 1.5, 2.0, 3.0];

/// The canvas a step must leave the layouts, give or take [`ALLOWANCE`], in layout units. Down to
/// nine tenths of it, 1440 by 864 (a 4:3 screen's width at 100%), every
/// screen stands whole: the HUD's rows along the top and the foot keep apart, the log window
/// narrowed beside the foot's widest (the spell bar's tab keys, or in gamepad mode the cross
/// hotbars, which also reach towards the main menu), character select and creation keep their
/// parts apart, and every window opens on the screen.
pub const CANVAS: (f32, f32) = (1600.0, 960.0);

/// How much of [`CANVAS`] a step must leave the layouts for the step to be offered.
pub const ALLOWANCE: f32 = 0.9;

/// The largest step a player who has not chosen one is drawn at.
pub const AUTOMATIC_LARGEST: f32 = 2.0;

/// A step as the options name it: `150%`.
#[must_use]
pub fn label(step: f32) -> String {
    format!(
        "{}%",
        dereth_primitives::num::to_i32((step * 100.0).round())
    )
}

/// The smallest window, in pixels across and down, that offers `step`.
#[must_use]
pub fn smallest_window(step: f32) -> (u32, u32) {
    let side = |canvas: f32| {
        u32::try_from(dereth_primitives::num::to_i32(
            (canvas * step * ALLOWANCE).round(),
        ))
        .unwrap_or(0)
    };
    (side(CANVAS.0), side(CANVAS.1))
}

/// Whether a window `window` pixels across and down offers `step`: 100% and less on every window.
#[must_use]
pub fn offered(step: f32, window: (f32, f32)) -> bool {
    if step <= 1.0 {
        return true;
    }
    let (w, h) = smallest_window(step);
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (w as f32, h as f32);
    window.0 >= w && window.1 >= h
}

/// The steps a window offers, in order.
#[must_use]
pub fn ladder(window: (f32, f32)) -> Vec<f32> {
    STEPS
        .iter()
        .copied()
        .filter(|s| offered(*s, window))
        .collect()
}

/// The step a player who has not chosen one is drawn at: the largest the window offers, up to
/// [`AUTOMATIC_LARGEST`].
#[must_use]
pub fn automatic(window: (f32, f32)) -> f32 {
    STEPS
        .iter()
        .copied()
        .filter(|s| *s <= AUTOMATIC_LARGEST && offered(*s, window))
        .fold(1.0, f32::max)
}

/// The step the interface is drawn at: the player's `chosen` while the window offers it, else the
/// automatic one.
#[must_use]
pub fn in_effect(chosen: Option<f32>, window: (f32, f32)) -> f32 {
    match chosen {
        Some(step) if offered(step, window) => step,
        _ => automatic(window),
    }
}

/// The smallest step the window does not offer, if there is one.
#[must_use]
pub fn first_withheld(window: (f32, f32)) -> Option<f32> {
    STEPS.iter().copied().find(|s| !offered(*s, window))
}

/// What the options say under the scale's choices, when there is something to say: why the step
/// the player chose is not the one drawn, or which steps a larger window would offer.
#[must_use]
pub fn note(chosen: Option<f32>, window: (f32, f32)) -> Option<String> {
    let needs = |step: f32| {
        let (w, h) = smallest_window(step);
        format!("a window of {w} \u{d7} {h} or larger")
    };
    if let Some(step) = chosen.filter(|s| !offered(*s, window)) {
        return Some(format!(
            "{} needs {}: {} until then.",
            label(step),
            needs(step),
            label(in_effect(chosen, window))
        ));
    }
    let step = first_withheld(window)?;
    let last = STEPS
        .last()
        .is_some_and(|s| (*s - step).abs() < f32::EPSILON);
    Some(if last {
        format!("{} needs {}.", label(step), needs(step))
    } else {
        format!("{} and larger need {}.", label(step), needs(step))
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    /// The monitors the ladder is checked on, each with the steps it offers and the one a player
    /// who has not chosen is drawn at.
    const MONITORS: [((f32, f32), &[f32], f32); 6] = [
        ((1280.0, 720.0), &[1.0], 1.0),
        ((1920.0, 1080.0), &[1.0], 1.0),
        ((2560.0, 1440.0), &[1.0, 1.5], 1.5),
        ((3440.0, 1440.0), &[1.0, 1.5], 1.5),
        ((3840.0, 2160.0), &[1.0, 1.5, 2.0], 2.0),
        ((5120.0, 2880.0), &[1.0, 1.5, 2.0, 3.0], 2.0),
    ];

    /// The system's display scalings the ladder is checked under.
    const SYSTEM_SCALES: [f32; 3] = [1.0, 1.5, 2.0];

    /// The pixels of a window the system sizes as `units` of its own at display scaling `scale`.
    fn pixels(units: (f32, f32), scale: f32) -> (f32, f32) {
        (units.0 * scale, units.1 * scale)
    }

    #[test]
    fn each_monitor_offers_the_steps_its_pixels_hold_and_a_new_player_gets_the_largest_up_to_200() {
        for (monitor, steps, auto) in MONITORS {
            for scale in SYSTEM_SCALES {
                // Full screen, the window is the monitor's pixels whatever the system's scaling;
                // a window the system sizes to its whole desktop, in its own units, is too.
                for window in [
                    monitor,
                    pixels((monitor.0 / scale, monitor.1 / scale), scale),
                ] {
                    assert_eq!(ladder(window), steps, "{monitor:?} at {scale}");
                    assert_eq!(automatic(window), auto, "{monitor:?} at {scale}");
                    assert_eq!(in_effect(None, window), auto, "{monitor:?} at {scale}");
                }
            }
        }
        // The same size in the system's units is more pixels at a larger scaling, and offers more.
        assert_eq!(ladder(pixels((1920.0, 1080.0), 1.0)), [1.0]);
        assert_eq!(ladder(pixels((1920.0, 1080.0), 1.5)), [1.0, 1.5]);
        assert_eq!(ladder(pixels((1920.0, 1080.0), 2.0)), [1.0, 1.5, 2.0]);
        // Maximised rather than full screen, the window is shorter than its monitor by the task
        // bar and its own frame, and offers the same.
        assert_eq!(automatic((2560.0, 1377.0)), 1.5);
        assert_eq!(automatic((3840.0, 2097.0)), 2.0);
        // 300% only where the window holds it, and never for a player who has not chosen it.
        let big = (6016.0, 3384.0);
        assert_eq!(ladder(big), STEPS);
        assert_eq!(automatic(big), 2.0);
    }

    #[test]
    fn each_step_is_offered_from_nine_tenths_of_the_canvas_at_that_step_and_not_a_pixel_before() {
        assert_eq!(smallest_window(1.5), (2160, 1296));
        assert_eq!(smallest_window(2.0), (2880, 1728));
        assert_eq!(smallest_window(3.0), (4320, 2592));
        for step in [1.5, 2.0, 3.0] {
            let (w, h) = smallest_window(step);
            #[allow(clippy::cast_precision_loss)]
            let (w, h) = (w as f32, h as f32);
            assert!(offered(step, (w, h)), "{step} at its smallest window");
            assert!(!offered(step, (w - 1.0, h)), "{step} a pixel too narrow");
            assert!(!offered(step, (w, h - 1.0)), "{step} a pixel too short");
        }
        assert!(offered(1.0, (640.0, 480.0)), "100% on any window");
        assert!(offered(0.5, (640.0, 480.0)), "and less");
        // A 16:10 laptop's 2560 by 1600 holds 150% and its 2880 by 1800 200%; a portrait 4K
        // screen is too narrow for 200%.
        assert_eq!(ladder((2560.0, 1600.0)), [1.0, 1.5]);
        assert_eq!(ladder((2880.0, 1800.0)), [1.0, 1.5, 2.0]);
        assert_eq!(ladder((2160.0, 3840.0)), [1.0, 1.5]);
    }

    #[test]
    fn a_chosen_step_is_drawn_while_the_window_offers_it_and_the_automatic_one_while_it_does_not() {
        let uhd = (3840.0, 2160.0);
        let fhd = (1920.0, 1080.0);
        assert_eq!(
            in_effect(Some(1.5), uhd),
            1.5,
            "a step below the automatic one"
        );
        assert_eq!(in_effect(Some(1.0), uhd), 1.0);
        assert_eq!(in_effect(Some(2.0), uhd), 2.0);
        assert_eq!(
            in_effect(Some(3.0), uhd),
            2.0,
            "300% not offered: the largest that is"
        );
        assert_eq!(in_effect(Some(2.0), fhd), 1.0, "the window made smaller");
        assert_eq!(
            in_effect(Some(0.5), fhd),
            0.5,
            "less than 100% on any window"
        );
        assert_eq!(
            in_effect(Some(1.25), (2560.0, 1440.0)),
            1.25,
            "a step the file holds"
        );
    }

    #[test]
    fn the_options_say_which_steps_a_larger_window_offers_and_why_a_chosen_one_is_not_drawn() {
        assert_eq!(
            note(None, (1920.0, 1080.0)).as_deref(),
            Some("150% and larger need a window of 2160 \u{d7} 1296 or larger.")
        );
        assert_eq!(
            note(None, (3840.0, 2160.0)).as_deref(),
            Some("300% needs a window of 4320 \u{d7} 2592 or larger.")
        );
        assert_eq!(
            note(Some(2.0), (1920.0, 1080.0)).as_deref(),
            Some("200% needs a window of 2880 \u{d7} 1728 or larger: 100% until then.")
        );
        assert_eq!(note(Some(1.5), (6016.0, 3384.0)), None, "nothing withheld");
        assert_eq!(label(1.5), "150%");
    }
}
