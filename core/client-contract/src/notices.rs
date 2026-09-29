//! The inbound **notice** queue — the mirror of [`crate::requests`].
//!
//! `crate::requests` carries UI -> client systems; this carries the other
//! direction, client systems -> UI, for the one system that needs it today:
//! the combat system's magic-action handler raises a magic notice and the spellcasting panel
//! receives it.
//!
//! ## Why a queue and not a call
//!
//! In the client, the cast-current-spell notice reaches
//! global event dispatch, which walks the elements registered for that notice id and calls each
//! one's handler. This build has **no notice bus**: `dereth_ui::NoticeBus`
//! exists, `UiSystem::send_notice` has one production caller and `Screen::on_notice` has zero
//! overrides, so every notice handler in the workspace is polled or hand-delivered instead.
//!
//! The producer here is `dereth_client::Interaction`, which holds `&mut dereth_client_model::World` and no
//! `UiSystem`; the consumer is a panel bound to a subtree, driven once a frame with both a
//! `UiSystem` and a `GameView`. So the notice is queued in the producer's frame slot and drained
//! in the consumer's, one step later — the same one-step deferral `crate::requests`,
//! `GamePlayScreen::panel_messages` and `trade_drops` all take, and for the same borrow reason.
//!
//! The one-frame lateness is worth stating rather than leaving to be discovered: a spell key
//! pressed on frame *n* moves the spell bar on frame *n + 1*. In retail the notice runs inside the
//! input dispatch. Nothing in this build can observe the difference — the bar is not read again
//! until it is drawn — but it is a deviation and it is declared here.
//!
//! ## The inbox is a `NoticeInbox`, owned by the UI that drains it
//!
//! The two queues are the fields of `NoticeInbox`, an ordinary value, not per-thread state, and
//! the one a UI drains is the `notice_inbox` field of its `dereth_ui::UiSystem`, next to the
//! request [`crate::requests::Outbox`]. A producer that holds no `UiSystem` keeps an inbox of its
//! own and its owner hands the notices over before the consuming panels run.

use crate::view::MagicNotice;

/// The notice queues a panel drains, as a value.
#[derive(Debug, Default)]
pub struct NoticeInbox {
    magic: Vec<MagicNotice>,
    /// A request to examine one spell.
    ///
    /// The one producer in the client is the client's
    /// right-press arm (notice parameter 8), which falls through to
    /// the spell-examine path when the row under the mouse is a *spell* row rather than an item
    /// row; the one consumer is the examination panel. Both ends are elements in the client;
    /// here the producer is a panel bound to
    /// the spellbook or the spell bar, which live on `dereth_client::hud::Hud`, and the consumer is
    /// `ExaminationPanel`, which lives on `GamePlayScreen`. Neither can call the other, so the
    /// notice takes the same one-frame queue every other cross-owner hop in this crate takes.
    examine_spell: Vec<u32>,
}

impl NoticeInbox {
    /// A private pair of queues, owned by whoever holds it.
    #[must_use]
    pub fn owned() -> Self {
        Self::default()
    }

    /// Append one notice from the magic-action handler's counterpart.
    pub fn emit(&mut self, n: MagicNotice) {
        self.magic.push(n);
    }

    /// Take everything queued so far, in emission order, and leave the queue empty.
    ///
    /// Drained by `dereth_ui_screens::panels::remaining::RemainingPanels::update`, which is the one place
    /// that holds `SpellcastingPanel`, a `UiSystem` and a `GameView` at the same time.
    #[must_use]
    pub fn take(&mut self) -> Vec<MagicNotice> {
        std::mem::take(&mut self.magic)
    }

    /// Append one examine-spell notice.
    ///
    /// A zero spell id is not queued: the client's spell-examine entry point returns immediately
    /// on a zero id, and the client's producer only reaches the call when
    /// the row's spell id is non-zero.
    pub fn emit_examine_spell(&mut self, spell: u32) {
        if spell == 0 {
            return;
        }
        self.examine_spell.push(spell);
    }

    /// Take every queued examine-spell notice, in emission order, and leave the queue empty.
    ///
    /// Drained by `dereth_ui_screens::panels::examination::ExaminationPanel::update`, which is the one place
    /// that holds `ExaminationPanel`, a `UiSystem` and a `GameView` at the same time.
    #[must_use]
    pub fn take_examine_spell(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.examine_spell)
    }

    /// How many examine-spell notices are queued, without taking them.
    #[must_use]
    pub fn examine_spell_len(&self) -> usize {
        self.examine_spell.len()
    }

    /// How many notices are queued, without taking them.
    #[must_use]
    pub fn len(&self) -> usize {
        self.magic.len()
    }

    /// Whether the magic-notice queue is empty, without taking it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.magic.is_empty()
    }

    /// Drop everything queued, in both queues. Used by tests between cases, and by the host on a
    /// mode switch — where the client's equivalent is that the element registered for the notice
    /// no longer exists.
    pub fn clear(&mut self) {
        self.magic.clear();
        self.examine_spell.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: [`crate::requests`]'s own contract, which this module mirrors — "appended in
    /// dispatch order and drained by the owner". The order is the observable part: two quickslot
    /// keys pressed in one frame must cast in the order they were pressed.
    #[test]
    fn notices_come_back_in_emission_order_and_the_queue_empties() {
        let mut inbox = NoticeInbox::owned();
        assert_eq!(inbox.len(), 0);
        inbox.emit(MagicNotice::CastQuickslotSpell { slot: 0 });
        inbox.emit(MagicNotice::NextSpellSelection);
        inbox.emit(MagicNotice::CastQuickslotSpell { slot: 3 });
        assert_eq!(inbox.len(), 3);
        assert_eq!(
            inbox.take(),
            vec![
                MagicNotice::CastQuickslotSpell { slot: 0 },
                MagicNotice::NextSpellSelection,
                MagicNotice::CastQuickslotSpell { slot: 3 },
            ]
        );
        assert_eq!(inbox.len(), 0, "taking empties it");
        assert!(
            inbox.take().is_empty(),
            "and a second take is not the first take again"
        );
    }
}
