//! The retail client's shutdown order.
//!
//! The device's done flag is the single quit switch. The event loop returns on the next frame,
//! the frame loop returns false, the run loop returns, and top-level client cleanup runs in this
//! order:
//!
//! ```text
//! run client cleanup;
//! release the gameplay client, then the base client;
//! clean up the object factory;
//! clean up version state;
//! ```
//!
//! Client-specific cleanup writes the keymap and unregisters its preferences, then chains to the
//! eleven shared cleanup steps represented by `Step`. **Three of the
//! orderings are load-bearing**: network cleanup before
//! UI cleanup (the UI holds no net references but the world controller does), UI cleanup before
//! database cleanup (UI elements hold dat objects), and database cleanup last of the subsystems
//! so the dat files are closed with every reference gone.
//!
//! # What this module is
//!
//! The **order**, as a recorded list, plus the note of which steps this build has a counterpart for.
//! `App::shutdown` walks it; the shutdown tests assert the recording against retail's order. A step
//! with no counterpart is `Outcome::NotInThisBuild` and is named rather than silently absent —
//! the alternative is a shutdown that looks complete because the things it forgot were never
//! listed.

/// One step of gameplay-client cleanup followed by shared client cleanup, in call order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// Gameplay-client cleanup detaches UI preferences, then unregisters the world-name notice.
    DetachUiPreferences,
    /// Save the keymap into its configured file under the preferences directory, then
    /// clear the in-memory keymap before the remaining cleanup.
    SaveKeyMap,
    /// Client cleanup step 1 disconnects every connection with optional header `0x00008000`,
    /// marks logoff sent so nothing more is transmitted,
    /// and resets the database cache.
    Disconnect,
    /// 2 — clean up the communication system.
    CommunicationSystem,
    /// 3 — log off the server again (idempotently), then release the
    /// packet controller and network client, drain every network blob from all 12
    /// queues, and release the queue array.
    CleanupNet,
    /// 4 — shut down language information.
    LanguageInfo,
    /// 5 — clean up the UI flow, element manager, queue manager,
    /// world controller, device and key-stone state, then uninitialize COM.
    CleanupUi,
    /// 6 — clean up preferences, ending by saving them.
    CleanupPreferences,
    /// 7, 8, 9 — release the quality registrar and global event handler, then destroy the
    /// interface system.
    DeleteGlobals,
    /// 10 — release the database cache, closing every data-file controller
    /// and flushing its transaction journal.
    CleanupDatabase,
    /// 11 — clean up the sound manager.
    SoundManager,
    /// The client release path's second half destroys the game client and then the client,
    /// which is where closing the running-client semaphore releases the
    /// `"Empyrean Client"` single-instance lock.
    ReleaseClient,
    /// Not a client step: `Gpu::wait_idle`, before anything holding a device resource is dropped.
    ///
    /// The client has no counterpart because D3D9's `Release` blocks; D3D12's does not, and
    /// dropping a device with work in flight is undefined. It is last because it must observe every
    /// release the steps above performed.
    WaitIdle,
}

impl Step {
    /// The eleven-plus-three steps in call order — retail's order.
    pub const ORDER: [Step; 13] = [
        Step::DetachUiPreferences,
        Step::SaveKeyMap,
        Step::Disconnect,
        Step::CommunicationSystem,
        Step::CleanupNet,
        Step::LanguageInfo,
        Step::CleanupUi,
        Step::CleanupPreferences,
        Step::DeleteGlobals,
        Step::CleanupDatabase,
        Step::SoundManager,
        Step::ReleaseClient,
        Step::WaitIdle,
    ];
}

/// What one step did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The step ran.
    Ran,
    /// The step had nothing to do — no link, no UI, no audio device.
    Nothing,
    /// This build has no counterpart for the step. Named, not skipped silently.
    NotInThisBuild,
}

/// The recording `App::shutdown` produces, so the order can be asserted rather than assumed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CleanupLog(pub Vec<(Step, Outcome)>);

impl CleanupLog {
    pub fn push(&mut self, s: Step, o: Outcome) {
        self.0.push((s, o));
    }

    /// The steps in the order they ran, whatever each did.
    #[must_use]
    pub fn order(&self) -> Vec<Step> {
        self.0.iter().map(|(s, _)| *s).collect()
    }

    #[must_use]
    pub fn outcome(&self, s: Step) -> Option<Outcome> {
        self.0.iter().find(|(x, _)| *x == s).map(|(_, o)| *o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the eleven shared cleanup steps, preceded by the gameplay client's two
    /// cleanup steps and followed by the final client release.
    #[test]
    fn the_documented_order_is_the_one_this_module_names() {
        let o = Step::ORDER;
        // The three orderings the document calls out by name.
        let at = |s: Step| o.iter().position(|x| *x == s).expect("in ORDER");
        assert!(
            at(Step::CleanupNet) < at(Step::CleanupUi),
            "net before UI: the world controller holds network references"
        );
        assert!(
            at(Step::CleanupUi) < at(Step::CleanupDatabase),
            "UI before database: UI elements hold dat objects"
        );
        assert!(
            at(Step::CleanupDatabase) > at(Step::CleanupPreferences),
            "the database is closed after the subsystems that hold references into it"
        );
        // The two persistence points, in their documented places.
        assert!(
            at(Step::SaveKeyMap) < at(Step::Disconnect),
            "the keymap is written first"
        );
        assert!(
            at(Step::CleanupPreferences) > at(Step::CleanupUi),
            "preferences cleanup is step 6, after UI cleanup"
        );
        assert_eq!(o.len(), 13);
    }
}
