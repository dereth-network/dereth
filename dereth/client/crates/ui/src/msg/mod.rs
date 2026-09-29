//! The event channels.
//!
//! The client has five independent message channels, and the game layer
//! uses each of them for something different:
//!
//! | Channel | Routing | Here |
//! |---|---|---|
//! | Element message | child → parent, per-element listener set, one dispatch per serial number | [`element`] |
//! | Global message | flat broadcast, no ordering guarantee | [`global`] |
//! | Notice | flat broadcast keyed by notice id | [`notice`] |
//! | Input action | handler stack, then the focus chain | [`crate::focus`] |
//! | Mouse | hit test, capture, hover, drag | [`crate::focus`] |

pub mod element;
pub mod global;
pub mod notice;

pub use element::{ElementMessage, ListenerId, MessagePoint};
pub use notice::{NoticeId, NoticePayload};

/// One queued delivery to a listener this crate does not own.
///
/// Elements are dispatched synchronously inside the caller's stack frame, exactly as
/// the element-message broadcast does. A listener that is **not** an element — a framework, a
/// flow controller or a screen — cannot be, because it owns the `&mut UiSystem` it would need. Those
/// deliveries are appended here in dispatch order and drained by the owner immediately after the
/// broadcast returns (`UiSystem::drain_outbox`). The *relative order* of external listeners, and
/// their order relative to the elements around them, is preserved.
#[derive(Debug, Clone, PartialEq)]
pub enum Delivery {
    Element {
        to: ListenerId,
        msg: ElementMessage,
    },
    Global {
        to: ListenerId,
        id: crate::MessageId,
        param: u32,
    },
    Notice {
        to: ListenerId,
        id: NoticeId,
        payload: NoticePayload,
    },
}
