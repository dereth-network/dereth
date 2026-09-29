//! The random draws the table logic makes, routed through one place.
//!
//! Hand-written, not ported. Unscripted draws go to `empyrean_common`'s port of
//! `ACE.Common.ThreadSafeRandom`.
//!
//! Tests script the draws with [`with_script`]: each queued [`Draw`] is consumed by the next call
//! and must be of the matching kind, so a test also pins the number and order of draws.

use std::cell::RefCell;
use std::collections::VecDeque;

use empyrean_common::thread_safe_random::ThreadSafeRandom;

/// One scripted draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Draw {
    /// The underlying .NET `Random.NextDouble()` value, in `[0, 1)`, for
    /// `ThreadSafeRandom.Next(float, float)`.
    Double(f64),
    /// The result of `ThreadSafeRandom.Next(int min, int max)` (inclusive of `max`).
    Int(i32),
}

thread_local! {
    static SCRIPT: RefCell<Option<VecDeque<Draw>>> = const { RefCell::new(None) };
}

/// Runs `f` with the draws on this thread taken from `draws`, in order. Panics if `f` makes a draw
/// the script does not have, or of the wrong kind, or leaves any unused.
pub fn with_script<R>(draws: &[Draw], f: impl FnOnce() -> R) -> R {
    SCRIPT.with(|s| *s.borrow_mut() = Some(draws.iter().copied().collect()));
    let out = f();
    let left = SCRIPT.with(|s| s.borrow_mut().take()).unwrap_or_default();
    assert!(left.is_empty(), "scripted draws left unused: {left:?}");
    out
}

fn scripted() -> Option<Draw> {
    SCRIPT.with(|s| {
        s.borrow_mut()
            .as_mut()
            .map(|q| q.pop_front().expect("more draws than the script holds"))
    })
}

/// `ThreadSafeRandom.Next(float min, float max)`: a `double` in `[min, max)`.
pub(crate) fn next_double(min: f32, max: f32) -> f64 {
    let next_double = match scripted() {
        Some(Draw::Double(d)) => d,
        Some(other) => panic!("scripted {other:?}, but a double draw was made"),
        None => return ThreadSafeRandom::next_float(min, max),
    };
    next_double * f64::from(max - min) + f64::from(min)
}

/// `ThreadSafeRandom.Next(int min, int max)`: an `int` in `[min, max]`.
pub(crate) fn next_int(min: i32, max: i32) -> i32 {
    match scripted() {
        Some(Draw::Int(i)) => {
            assert!(
                (min..=max).contains(&i),
                "scripted {i} outside [{min}, {max}]"
            );
            i
        }
        Some(other) => panic!("scripted {other:?}, but an int draw was made"),
        None => ThreadSafeRandom::next(min, max),
    }
}
