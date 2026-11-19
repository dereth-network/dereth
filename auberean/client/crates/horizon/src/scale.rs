//! The interface scale: which of Horizon's steps a window offers, and the scale it is drawn at.
//!
//! Every layout is drawn for a canvas 1080 lines tall, a layout unit to a pixel at 100%; a window
//! shorter than that shrinks the layouts to fit it. A larger step draws everything larger, and a
//! step is offered only on a window that, at that step, still holds nine tenths of [`CANVAS`]: 150%
//! from 2160 by 1296 pixels, 200% from 2880 by 1728, 300% from 4320 by 2592. 100% is offered on
//! every window.
//!
//! A player who has not chosen a step (Auto, the settings window's first choice) is drawn at the
//! largest step the window offers, up to [`AUTOMATIC_LARGEST`]. A step the player chose is drawn
//! at while the window offers it, and the automatic step while it does not, the choice kept for
//! when it does again. Both follow the window from frame to frame, as it is resized.
//!
//! The window is measured in its own pixels, as the shell hands the interface its size, so the
//! system's display scaling is already in it.

/// The steps, in order.
pub const STEPS: [f32; 4] = [1.0, 1.5, 2.0, 3.0];

/// The canvas a step must leave the layouts, give or take [`ALLOWANCE`], in layout units.
pub const CANVAS: (f32, f32) = (1600.0, 960.0);

/// How much of [`CANVAS`] a step must leave the layouts for the step to be offered.
pub const ALLOWANCE: f32 = 0.9;

/// The largest step drawn at without the player choosing it.
pub const AUTOMATIC_LARGEST: f32 = 2.0;

/// A step as the settings name it: `150%`.
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

/// The step drawn at without the player choosing one: the largest the window offers, up to
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

/// What the settings say under the scale's choices, when there is something to say: why the step
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
    let step = STEPS.iter().copied().find(|s| !offered(*s, window))?;
    let last = STEPS
        .last()
        .is_some_and(|s| (*s - step).abs() < f32::EPSILON);
    Some(if last {
        format!("{} needs {}.", label(step), needs(step))
    } else {
        format!("{} and larger need {}.", label(step), needs(step))
    })
}

/// Screen pixels per layout unit on a window of `screen` pixels, at the step `chosen` (none:
/// Auto) as the window offers it, shrunk on a window shorter than 1080 lines (to half at most).
#[must_use]
pub fn layout_scale_at(screen: (f32, f32), chosen: Option<f32>) -> f32 {
    (screen.1 / 1080.0).clamp(0.5, 1.0) * in_effect(chosen, screen)
}

/// Screen pixels per layout unit on a window of `screen` pixels, at the automatic step.
#[must_use]
pub fn layout_scale(screen: (f32, f32)) -> f32 {
    layout_scale_at(screen, None)
}
