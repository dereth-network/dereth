//! What a soak run must hold: the world never panicked and ran throughout, every player entered and
//! stayed, the world's steps kept within their budget at the run's load, and the world holds no more
//! objects once everyone has left than it did before they came, but for the corpses still lying.

use crate::metrics::Measured;

/// The most of the sampled steps that may overrun their budget (a sustained overrun fails; a spike
/// does not), unless `YALAIN_SOAK_OVERRUN_SHARE` says otherwise.
pub const OVERRUN_SHARE: f64 = 0.05;

/// The objects the world may hold past its count before the players came, beyond the corpses still
/// lying: a few, or a share of the world.
pub const LEAK_SLACK: u64 = 20;
pub const LEAK_SHARE: f64 = 0.02;

/// What a run held and what it did not: each failure in words.
#[must_use]
pub fn check(m: &Measured, overrun_share: f64) -> Vec<String> {
    let mut failed = Vec::new();
    if m.panicked {
        failed.push("the world panicked".to_owned());
    }
    if m.in_world_at_end != m.players {
        failed.push(format!(
            "{} of {} players were in the world at the end",
            m.in_world_at_end, m.players
        ));
    }
    let (steps, overruns) = m.steps();
    #[allow(clippy::cast_precision_loss)]
    let share = if steps == 0 {
        0.0
    } else {
        overruns as f64 / steps as f64
    };
    if share > overrun_share {
        failed.push(format!(
            "{overruns} of {steps} steps overran ({:.1}%, more than {:.1}%) with {} players",
            share * 100.0,
            overrun_share * 100.0,
            m.players
        ));
    }
    // Every corpse made may still lie: each creature the players saw go, at most.
    let corpses = m.totals().kills_seen;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let slack = LEAK_SLACK.max((m.objects_before as f64 * LEAK_SHARE) as u64);
    if m.objects_after > m.objects_before + corpses + slack {
        failed.push(format!(
            "the world holds {} objects after everyone left, against {} before (corpses up to {corpses}, \
             slack {slack})",
            m.objects_after, m.objects_before
        ));
    }
    failed
}
