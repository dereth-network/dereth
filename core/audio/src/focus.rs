//! The two focus mechanisms, which a modern audio API gives you neither of.
//!
//! The two mechanisms preserve buffer ownership and focus transitions independently.
//!
//! 1. **The preference.** "Play sound only when active" (default true) is tested at the top
//!    of every one of the eight public play functions *and* again in the internal play path:
//!    If sound-only-when-active is enabled and the app is inactive, playback returns early. It suppresses **new**
//!    sounds only. Nothing is stopped, nothing is queued, and already-playing sounds carry on as far
//!    as the client is concerned.
//! 2. **DirectSound's own rule.** No secondary buffer carries `DSBCAPS_GLOBALFOCUS` or
//!    `DSBCAPS_STICKYFOCUS` (the client asks for flags `0x100E2`), so DirectSound
//!    silenced every buffer for the whole time the client was not the foreground application,
//!    independently of the preference. That is why the already-playing sounds were inaudible in
//!    practice even though the client never stopped them.
//!
//! Both are needed. With only (1), sound keeps coming out of a background window. With only (2), the
//! ambient scheduler keeps firing and its queue behaves differently on the way back. With neither in
//! the right shape, regaining focus produces a pile-up.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// That preference plus the explicit master mute.
#[derive(Debug, Clone)]
pub struct Focus {
    /// Whether the client is the active application, maintained by the window's activate and
    /// deactivate handlers from `WM_ACTIVATEAPP`.
    active: bool,
    /// The mute DirectSound applied for free. Shared with the mixer, which reads it in its callback.
    muted: Arc<AtomicBool>,
}

impl Default for Focus {
    fn default() -> Self {
        Self::new(Arc::new(AtomicBool::new(false)))
    }
}

impl Focus {
    #[must_use]
    pub fn new(muted: Arc<AtomicBool>) -> Self {
        // The client starts foreground; its active-app flag is set by the first `WM_ACTIVATEAPP`.
        muted.store(false, Ordering::Relaxed);
        Self {
            active: true,
            muted,
        }
    }

    /// The flag the mixer reads.
    #[must_use]
    pub fn mute_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.muted)
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    #[must_use]
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    /// `WM_ACTIVATEAPP`. Losing focus mutes the output; regaining it unmutes.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        self.muted.store(!active, Ordering::Relaxed);
    }

    /// When sound-only-when-active is enabled, an inactive app returns before playback.
    ///
    /// This is the gate on **starting** a sound. It is deliberately *not* a gate on the ambient
    /// scheduler's own bookkeeping: the scheduler keeps draining the queue and
    /// re-inserting while the window is inactive, so the schedule does not pause and there is
    /// nothing to catch up on when focus returns.
    #[must_use]
    pub fn may_start(&self, only_when_active: bool) -> bool {
        !(only_when_active && !self.active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered focus check and the preference test at the head of the internal play
    /// path.
    #[test]
    fn the_preference_suppresses_new_sounds_only_while_inactive() {
        let mut f = Focus::default();
        assert!(f.may_start(true));
        f.set_active(false);
        assert!(
            !f.may_start(true),
            "the default preference suppresses new sounds"
        );
        assert!(
            f.may_start(false),
            "with the preference off, new sounds still start"
        );
        f.set_active(true);
        assert!(f.may_start(true));
    }

    /// The second mechanism: losing focus mutes the master output regardless of the preference,
    /// because no secondary buffer has `DSBCAPS_GLOBALFOCUS`.
    #[test]
    fn losing_focus_mutes_the_output_even_when_the_preference_is_off() {
        let mut f = Focus::default();
        assert!(!f.is_muted());
        f.set_active(false);
        assert!(
            f.is_muted(),
            "DirectSound silenced every buffer while inactive"
        );
        // The preference has nothing to do with it.
        assert!(f.may_start(false));
        assert!(f.is_muted());
        f.set_active(true);
        assert!(!f.is_muted());
    }

    /// The mute flag is shared, so the mixer sees the change without being told.
    #[test]
    fn the_mute_flag_is_shared_with_the_mixer() {
        let flag = Arc::new(AtomicBool::new(false));
        let mut f = Focus::new(Arc::clone(&flag));
        f.set_active(false);
        assert!(flag.load(Ordering::Relaxed));
    }
}
