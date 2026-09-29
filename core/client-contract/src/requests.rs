//! The outbound request queue.
//!
//! ## Why a queue and not a return value
//!
//! In the client a panel sends a UI notice or performs a game action directly from inside an
//! element-message handler. A `dereth_ui::Element` handler is handed only
//! `&mut UiSystem` (`ElemCtx`), because the behaviour object has been lifted out of its arena slot
//! for the duration of the call — there is nowhere to put a `&mut dyn Host` without aliasing the
//! very tree the handler is walking.
//!
//! So requests are appended to a queue and the host drains it after the handler returns, exactly
//! as `dereth_ui`'s own `Delivery` outbox does for non-element listeners.
//!
//! ## The queue is an `Outbox`, owned by the UI it serves
//!
//! A bare `thread_local! { RefCell<Vec<UiRequest>> }` that every panel reached through a free
//! function would make a screen impossible to drive without process-wide state, which is the
//! obstacle to two presentations or a parallel test matrix.
//!
//! So the queue is `Outbox`, an ordinary value, and the one a UI emits into is the `requests`
//! field of its `dereth_ui::UiSystem`. That is the owner because the queue has to be reachable from
//! everything that emits in the client's order: a screen's handlers, the panels they call, and
//! element behaviours whose hooks run inside the UI's own move, resize and visibility calls (the
//! floating toolbar's placement writes). One queue per UI, appended to in execution order and
//! drained by the host at the same points of the frame, is the whole contract.

use crate::view::UiRequest;

/// The request queue a panel appends to, as a value.
#[derive(Debug, Default)]
pub struct Outbox(Vec<UiRequest>);

impl Outbox {
    /// An empty queue, owned by whoever holds it.
    #[must_use]
    pub fn owned() -> Self {
        Self(Vec::new())
    }

    /// Append one request. Called from panel message handlers.
    pub fn emit(&mut self, r: UiRequest) {
        self.0.push(r);
    }

    /// Take everything queued so far, in emission order, and leave the queue empty.
    ///
    /// The host calls this once per frame, after `UiFlow::frame`.
    #[must_use]
    pub fn take(&mut self) -> Vec<UiRequest> {
        std::mem::take(&mut self.0)
    }

    /// Take only requests appended since a caller's [`Self::len`] boundary, preserving its earlier
    /// queue. The bounded subscriber must not clear/drain that queue while producing requests.
    #[must_use]
    pub fn take_since(&mut self, boundary: usize) -> Vec<UiRequest> {
        self.0.split_off(boundary)
    }

    /// Extract only local `PlayerModule` placement writes, preserving every other request in
    /// order. The host consumes these before PM restoration, and drops old writes when a new
    /// authoritative `PlayerDescription` replaces that module. They must not wait for the next
    /// interaction frame.
    #[must_use]
    pub fn take_placement_updates(&mut self) -> Vec<UiRequest> {
        let mut placements = Vec::new();
        let mut others = Vec::new();
        for request in std::mem::take(&mut self.0) {
            if is_local_module_write(&request) {
                placements.push(request);
            } else {
                others.push(request);
            }
        }
        self.0 = others;
        placements
    }

    /// Extract only completed slumlord confirmation answers, preserving every other request in
    /// order. `TargetedDialogs` emits these before `Hud::drive`; the panel owns the current payment
    /// rows and must re-check them before it can turn a Yes into `HousePayment`.
    #[must_use]
    pub fn take_house_payment_confirmation_answers(&mut self) -> Vec<(bool, Option<bool>)> {
        let mut answers = Vec::new();
        let mut others = Vec::new();
        for request in std::mem::take(&mut self.0) {
            match request {
                UiRequest::HousePaymentConfirmationAnswer { rent, confirmed } => {
                    answers.push((rent, confirmed));
                }
                other => others.push(other),
            }
        }
        self.0 = others;
        answers
    }

    /// Drop everything queued. Used by tests between cases, and by the host on a mode switch, where
    /// the client's equivalent is that the objects that would have made the calls no longer exist.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// How many requests are queued, without taking them.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the queue is empty, without taking it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[must_use]
pub fn is_numeric_placement_update(request: &UiRequest) -> bool {
    matches!(
        request,
        UiRequest::SetChatWindowOption {
            property: 0x1000_0086..=0x1000_008A,
            ..
        }
    )
}

/// Every request whose whole effect is a write into the retained `PlayerModule` — the placement
/// blob **and** the Chat Options page's two kinds of write.
///
/// They belong together because they are the same code in the client:
/// the chat-window option write and the general option write both end in the player module's own
/// changed hook, which raises a **local** notice
/// and sets the 480-second deferred-save flag. None of the three puts a byte on the wire, and all
/// three have to land before the frame's `PlayerModule` read-back or the page would show the
/// value it just replaced.
#[must_use]
pub fn is_local_module_write(request: &UiRequest) -> bool {
    is_numeric_placement_update(request)
        || matches!(
            request,
            UiRequest::SetChatWindowTitle { .. }
                | UiRequest::SetChatWindowFilter { .. }
                | UiRequest::SetChatOpacity { .. }
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::ObjectId;

    /// Oracle: `dereth_ui::msg::Delivery`'s contract — "appended in dispatch order and drained by the
    /// owner immediately after the broadcast returns". The order is the observable part.
    #[test]
    fn requests_come_back_in_emission_order_and_the_queue_empties() {
        let mut out = Outbox::owned();
        out.emit(UiRequest::Select(ObjectId(1)));
        out.emit(UiRequest::Use(ObjectId(2)));
        assert_eq!(out.len(), 2);
        let got = out.take();
        assert_eq!(
            got,
            vec![UiRequest::Select(ObjectId(1)), UiRequest::Use(ObjectId(2))]
        );
        assert_eq!(out.len(), 0);
        assert!(out.take().is_empty());
    }

    #[test]
    fn subscriber_tail_does_not_steal_the_preceding_queue() {
        let mut out = Outbox::owned();
        out.emit(UiRequest::Select(ObjectId(1)));
        let boundary = out.len();
        out.emit(UiRequest::QueryHealth(ObjectId(0)));
        out.emit(UiRequest::QueryHealth(ObjectId(2)));
        assert_eq!(
            out.take_since(boundary),
            vec![
                UiRequest::QueryHealth(ObjectId(0)),
                UiRequest::QueryHealth(ObjectId(2))
            ]
        );
        assert_eq!(out.take(), vec![UiRequest::Select(ObjectId(1))]);
        assert!(out.take_since(0).is_empty());
    }
}
