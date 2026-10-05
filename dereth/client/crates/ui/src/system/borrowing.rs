//! Behavior borrowing and deferred updates.

use crate::{ElemHandle, Element, StateId, UiSystem};

impl UiSystem {
    // ---- behaviour borrowing ------------------------------------------------------------------

    pub(crate) fn take_behaviour(&mut self, h: ElemHandle) -> Option<Box<dyn Element>> {
        let b = self.node_mut(h).and_then(|n| n.behaviour.take());
        if b.is_some() {
            self.lifted += 1;
        }
        b
    }

    pub(crate) fn put_behaviour(&mut self, h: ElemHandle, b: Box<dyn Element>) {
        if let Some(n) = self.node_mut(h) {
            n.behaviour = Some(b);
        }
        self.lifted = self.lifted.saturating_sub(1);
        if self.lifted == 0 {
            // Before the states: a state change can raise another `on_set_attribute`, and the queue
            // below must not be left holding a handle across it.
            self.flush_deferred_relinquish();
            self.flush_deferred_mouse_visibility();
            self.flush_deferred_states();
        } else {
            // A restored child is safe even while its caller (e.g. GroupBox) is still lifted.
            // Finish this child's derived restates before the caller makes its next state change:
            // A group box can clear and reselect the SAME child. Waiting
            // for the parent would leave states 1/6 queued against the final toggle value,
            // recursively alternating the button's state-change attribute writes without finishing.
            // Do not drain other handles: an ancestor's behaviour can still be absent.
            self.flush_deferred_states_for(h);
        }
    }

    /// Relinquish focus, deferred until the element's behaviour is back in
    /// its slot. It is the same queue-shaped problem as
    /// [`Self::update_mouse_visibility`]'s.
    ///
    /// The text element's set-editable and set-selectable relinquish from
    /// *inside* `on_set_attribute`, so in this crate they would fire while the text element's own
    /// behaviour is lifted out of the arena. The manager's set-focus-element broadcasts `0x2F` with
    /// `p1 == 0` on the element it is taking the caret from, and
    /// the text element's own message handler's losing arm is where the client
    /// unregisters the caret tick and deselects — on **that same element**.
    /// Delivered synchronously the message would find the slot empty and both would be skipped,
    /// leaving a box that is no longer focused still blinking and still holding a highlight.
    pub(crate) fn queue_relinquish_focus(&mut self, h: ElemHandle) {
        self.deferred_relinquish.push(h);
    }

    pub(crate) fn flush_deferred_relinquish(&mut self) {
        for h in std::mem::take(&mut self.deferred_relinquish) {
            self.relinquish_focus(h);
        }
    }

    /// Apply the [`Self::update_mouse_visibility`] calls that were made while their
    /// element's behaviour was lifted.
    pub(crate) fn flush_deferred_mouse_visibility(&mut self) {
        for h in std::mem::take(&mut self.deferred_mouse_visibility) {
            self.update_mouse_visibility(h);
        }
    }

    /// Set an element state after its target behavior is back in its slot.
    ///
    /// **Why a queue.** A widget runs with its behaviour object moved out of the arena, which is
    /// this crate's stand-in for the client's in-flight listener iteration; a state change performed *inside* that window
    /// diffs the property set and calls `on_set_attribute` for every change, and the subclass half
    /// of `on_set_attribute` would be skipped because the slot is empty. That is not theoretical:
    /// **670 of the 3,275 state records in the gameplay tree carry properties**, and states 1, 3
    /// and 6 of the toolbar's buttons carry attribute 0x1B (the font-colour array), so a button
    /// that changed state from inside its own mouse-down would keep the wrong caption colour.
    /// The queued setter is called from exactly those five places.
    pub(crate) fn queue_set_state(&mut self, h: ElemHandle, s: StateId) {
        self.deferred_states.push((h, s));
    }

    pub(crate) fn flush_deferred_states_for(&mut self, h: ElemHandle) {
        while let Some(i) = self
            .deferred_states
            .iter()
            .position(|(target, _)| *target == h)
        {
            let (_, state) = self.deferred_states.remove(i);
            self.set_state(h, state);
        }
    }

    pub(crate) fn flush_deferred_states(&mut self) {
        // Drained from the front: a state change can queue another, and the client applies them in
        // the order they were asked for.
        let mut guard = 0;
        while !self.deferred_states.is_empty() && guard < 256 {
            guard += 1;
            let (h, s) = self.deferred_states.remove(0);
            self.set_state(h, s);
        }
    }
}
