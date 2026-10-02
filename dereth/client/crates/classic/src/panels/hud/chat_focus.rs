//! The chat destination menu. Disabling a row resets an active destination to All; the rows for
//! the selected target are enabled separately, from the talk target.
use dereth_client_model::chat::{TalkFocus, TalkFocusNotice};

#[derive(Debug)]
pub struct ChatFocusState {
    enabled: [bool; 7],
}
impl Default for ChatFocusState {
    fn default() -> Self {
        Self {
            enabled: [true, true, false, false, false, false, false],
        }
    }
}
impl ChatFocusState {
    /// One talk-focus notice, in delivery order; `current` is the talk focus now. True when the
    /// talk focus falls back to All: its row was just switched off. A disable followed by an
    /// enable still switches the active destination to All.
    pub fn answer(&mut self, current: TalkFocus, notice: TalkFocusNotice) -> bool {
        let raw = notice.focus as usize;
        if !(1..=7).contains(&raw) {
            return false;
        }
        let previous = std::mem::replace(&mut self.enabled[raw - 1], notice.enabled);
        previous && !notice.enabled && current == notice.focus
    }
    /// Target loss has the same fallback even though the model's Selected bit remains enabled:
    /// true when the talk focus `current` must fall back to All.
    #[must_use]
    pub fn target_lost(current: TalkFocus, present: bool) -> bool {
        !present && current == TalkFocus::Selected
    }
    #[must_use]
    pub fn snapshot(&self, current: TalkFocus) -> (u8, [bool; 7]) {
        (current as u8, self.enabled)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn disable_then_enable_retains_all_fallback_and_ignores_later_channels() {
        let mut state = ChatFocusState::default();
        let notice = |focus, enabled| TalkFocusNotice {
            focus,
            enabled,
            is_olthoi: false,
        };
        assert!(!state.answer(TalkFocus::All, notice(TalkFocus::Fellowship, true)));
        assert!(state.answer(TalkFocus::Fellowship, notice(TalkFocus::Fellowship, false)));
        assert!(!state.answer(TalkFocus::All, notice(TalkFocus::Fellowship, true)));
        assert!(!state.answer(TalkFocus::All, notice(TalkFocus::General, true)));
        assert_eq!(
            state.snapshot(TalkFocus::All),
            (1, [true, true, true, false, false, false, false])
        );
    }
    #[test]
    fn target_loss_resets_selected_without_affecting_other_destinations() {
        assert!(!ChatFocusState::target_lost(TalkFocus::Selected, true));
        assert!(ChatFocusState::target_lost(TalkFocus::Selected, false));
        assert!(!ChatFocusState::target_lost(TalkFocus::Allegiance, false));
    }
}
