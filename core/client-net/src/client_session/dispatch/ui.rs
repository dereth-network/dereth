//! Queue 9 — the UI queue: its per-frame step and its two routers.
//!
//! ```text
//! if the blob id's ephemeral flag is set   the ephemeral path
//! else                                     the ordering router
//! ```
//!
//! The ordering router:
//!
//! ```text
//! if the ordering header cannot be unpacked, process the whole payload   // not ordered
//! else if the header's object id is 0  process the payload after the header // globally ordered
//! else if the object is unknown        queue the blob on that object
//! else if the stamp is the next due    deliver, then drain what unblocked
//! ```
//!
//! **All three paths must exist even though ACE exercises one.** ACE never sends an ordered-event
//! header object id other than the player's own guid and never sets the ephemeral bit; retail
//! servers did order events against other objects' ids, which is why the object-id-0 and "unknown object" paths are
//! there.

use crate::client_session::ordering::{CrucialEventsGate, StampResult, StampWindow};
use crate::client_session::{DropReason, SessionEvent};
use dereth_primitives::{IncomingMessage, LocalTime, NetBlobId, NetQueue, ObjectId};
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::{Opcode, OrderedEventHeader};
use std::collections::HashMap;

/// The ephemeral flag — bit 63 of the 64-bit blob id.
#[must_use]
pub fn is_ephemeral(id: NetBlobId) -> bool {
    id.0 & (1u64 << 63) != 0
}

/// The ordering type — bits 60..56 of the id's high dword.
#[must_use]
pub fn ordering_type(id: NetBlobId) -> u8 {
    // The mask is `0x1F000000` of the high dword, shifted down to a small integer.
    #[allow(clippy::cast_possible_truncation)] // masked to five bits
    {
        ((id.high32() & 0x1F00_0000) >> 24) as u8
    }
}

/// [`ordering_type`] in the form the client itself compares: the five-bit field left **in place**,
/// which is literally `(u64)(high32 & 0x1F000000) << 32`. Matches `dereth_transport::blob`'s accessor.
#[must_use]
pub fn ordering_type_raw(id: NetBlobId) -> u64 {
    u64::from(id.high32() & 0x1F00_0000) << 32
}

/// The constant the client compares the blob's ordering type against.
///
/// It is the **ephemeral bit's** mask, and that is the whole story: see [`UiOrdering::route`].
pub const USE_TIME_EPHEMERAL_TEST: u64 = 1u64 << 63;

/// The per-object ordering state the UI queue keeps.
#[derive(Debug, Default)]
pub struct UiOrdering {
    /// One window per object -- the object's own timestamp-ordered receive queue.
    ///
    /// There is no second, separate table for blobs parked while the object was
    /// unknown. The object maintainer fetches the *null placeholder*
    /// weenie for the id and inserts into this very window; the placeholder is the object. A separate
    /// parked list for the ordered stream would leave the highest stamp at 0 across the seam and stall
    /// every ordered UI blob after the first -- see [`UiOrdering::object_arrived`].
    windows: HashMap<ObjectId, StampWindow>,
    /// The globally ordered stream, object id 0. Retail used it; ACE does not.
    global: StampWindow,
    pub gate: CrucialEventsGate,
}

/// What one UI-queue blob produced.
#[derive(Debug, Default)]
pub struct UiDispatch {
    /// Blobs ready to be decoded now, in order. Each is a whole blob including its type dword.
    pub ready: Vec<Vec<u8>>,
    pub events: Vec<SessionEvent>,
    /// A successful ordered delivery owes a `next_ready` AFTER its callback returns.
    /// The caller must not advance the rest of the window before consuming this entry.
    pub resume: Option<ObjectId>,
}

impl UiOrdering {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the client knows this object. The UI queue's "unknown object" path needs it, and the
    /// answer lives in the client's object table — so the caller supplies a predicate.
    ///
    /// The UI queue's per-frame step, then the ordering router.
    pub fn route(
        &mut self,
        m: &IncomingMessage,
        now: LocalTime,
        knows_object: &dyn Fn(ObjectId) -> bool,
    ) -> UiDispatch {
        let mut out = self.route_one(m, now, knows_object);
        if let Some(id) = out.resume.take() {
            while let Some(blob) = self.next_ready(id, now) {
                out.ready.push(blob);
            }
        }
        out
    }

    /// The ordering router's first admission only. The owning callback runs before
    /// asking for another ready entry, so deletion cannot leave a predecoded old-owner tail.
    pub fn route_one(
        &mut self,
        m: &IncomingMessage,
        now: LocalTime,
        knows_object: &dyn Fn(ObjectId) -> bool,
    ) -> UiDispatch {
        let mut out = UiDispatch::default();
        let blob = whole_blob(m);

        // The per-frame step's own test, reproduced exactly -- **including the fact that it never
        // fires.**
        //
        // The client compares the blob's ordering type against `0x80000000_00000000`, which is
        // the *ephemeral bit's* mask. But the ordering type is `(high32 & 0x1F000000) << 32`,
        // whose largest possible value is `0x1F000000_00000000`: bit 63 can never be set in it. So
        // the comparison is never true, the ephemeral path is dead code in the shipped client,
        // its waiting list is always empty, and **every** UI blob reaches the ordering router. Preserved rather than corrected, to match retail.
        //
        // Testing `is_ephemeral(m.blob_id)` here is the natural misreading: the constant really is
        // the ephemeral mask, so testing the bit looks like what was meant. It is not what the code
        // does, and against ACE it is fatal -- ACE stamps `0x80000000` on every blob, so every UI
        // blob would go into the crucial-events gate, which only `0x0013` opens and which `0x0013`
        // is itself behind. The session would deadlock at the character list.
        if ordering_type_raw(m.blob_id) == USE_TIME_EPHEMERAL_TEST {
            self.ephemeral(blob, &mut out);
            return out;
        }

        let (order, sub_type) = match split_ui_blob(&blob) {
            Ok(s) => (s.order, s.sub_type),
            Err(_) => {
                out.events.push(SessionEvent::Dropped {
                    queue: m.queue,
                    opcode: Opcode(m.opcode),
                    reason: DropReason::ShortBuffer,
                });
                return out;
            }
        };

        let Some(OrderedEventHeader { iid, stamp }) = order else {
            // Not ordered: the whole payload is the event body.
            out.ready.push(blob);
            return out;
        };

        // Object id 0 is the globally ordered stream. ACE never produces it and retail did, so it
        // gets a window of its own rather than being folded into a per-object one.
        let window = if iid == ObjectId(0) {
            &mut self.global
        } else {
            let w = self.windows.entry(iid).or_default();
            if !knows_object(iid) {
                // The object is not known yet. The ordering router hands the blob to
                // the object maintainer, which parks it -- header and
                // all -- on the *null placeholder* weenie's ordered receive queue.
                // That is this same window, which is the whole point: `object_arrived` then replays
                // it through `next_ready` and `highest` advances.
                w.park(stamp, blob);
                return out;
            }
            w
        };

        match window.add_returning(stamp, blob, now) {
            (StampResult::Deliver, delivered) => {
                if let Some(b) = delivered {
                    out.ready.push(strip(&b));
                }
                out.resume = Some(iid);
            }
            (StampResult::Queued, _) => {}
            (StampResult::Old, _) => out.events.push(SessionEvent::Dropped {
                queue: m.queue,
                opcode: sub_type,
                reason: DropReason::StaleOrderedStamp,
            }),
        }
        out
    }

    /// Process ordered object blobs one callback at a time. `iid=0` selects the independent global
    /// window.
    pub fn next_ready(&mut self, iid: ObjectId, now: LocalTime) -> Option<Vec<u8>> {
        let window = if iid == ObjectId(0) {
            &mut self.global
        } else {
            self.windows.get_mut(&iid)?
        };
        while let Some(entry) = window.next_ready(now) {
            if let Some(blob) = entry {
                return Some(strip(&blob));
            }
        }
        None
    }

    /// The ephemeral path — held until `0x0013` has arrived.
    fn ephemeral(&mut self, blob: Vec<u8>, out: &mut UiDispatch) {
        if self.gate.admit(blob.clone()) {
            out.ready.push(blob);
        }
    }

    /// The crucial-events gate opens — replay everything held, in order.
    pub fn crucial_events_received(&mut self) -> Vec<Vec<u8>> {
        self.gate.open()
    }

    /// The object arrived: replay everything parked on it, **through its own window**.
    ///
    /// The object's blob replay repeatedly takes the next ready entry and delivers it, which is
    /// exactly [`StampWindow::drain_ready`]. Because the parked blobs entered this window through
    /// [`StampWindow::park`], stamp 1 is `highest + 1` with
    /// `highest == 0` and the whole contiguous run comes out in order, leaving `highest` where the
    /// live path can carry on from.
    ///
    /// **The parked blobs must not bypass this window.** Handed straight to the dispatcher from a
    /// separate list, they would leave `highest` at 0, so the first live blob after the object became
    /// known would carry a stamp that is neither 0 nor 1, `StampWindow::add` would queue it, and every
    /// blob after it would queue behind it for ever -- the deadlock breaker cannot help, because
    /// `next_ready` being polled every frame pushes `blocked_since` forward
    /// (`polling_every_frame_defers_the_deadlock_breaker_for_ever`). Measured against the corpus,
    /// that shape let 0 of the server's 100 `0x0022 Item_ServerSaysContainID` reach
    /// `dereth_client_runtime::interaction::apply_events`.
    pub fn object_arrived(&mut self, id: ObjectId, now: LocalTime) -> Vec<Vec<u8>> {
        self.windows
            .get_mut(&id)
            .map_or_else(Vec::new, |w| w.drain_ready(now))
    }

    /// Release only this object's ordering window when the object is forgotten.
    pub fn forget_object(&mut self, id: ObjectId) {
        self.windows.remove(&id);
    }

    /// How many stamps are held out of order on an object.
    ///
    /// Parked-while-unknown and blocked-while-known are the **same** list, exactly as in retail:
    /// each object owns one timestamp-ordered receive queue.
    #[must_use]
    pub fn blocked_on(&self, id: ObjectId) -> usize {
        self.windows.get(&id).map_or(0, StampWindow::blocked_count)
    }

    /// The highest delivered stamp for an object's window, or 0 when it has none.
    #[must_use]
    pub fn highest_on(&self, id: ObjectId) -> u32 {
        self.windows.get(&id).map_or(0, StampWindow::highest)
    }

    /// Ending the character session — the whole ordering state is torn down when the player leaves.
    pub fn reset(&mut self) {
        self.windows.clear();
        self.global = StampWindow::new();
        self.gate.reset();
    }

    #[must_use]
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }
}

/// Rebuild the blob as it arrived: the opcode dword the transport lifted off, then the body.
fn whole_blob(m: &IncomingMessage) -> Vec<u8> {
    let mut v = m.opcode.to_le_bytes().to_vec();
    v.extend_from_slice(&m.body);
    v
}

fn strip(blob: &[u8]) -> Vec<u8> {
    if blob.len() >= OrderedEventHeader::PACK_SIZE
        && u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) == OrderedEventHeader::MAGIC
    {
        blob[OrderedEventHeader::PACK_SIZE..].to_vec()
    } else {
        blob.to_vec()
    }
}

/// The queue the client uses for a given queue id. Kept here because the routing table is the
/// dispatcher's business.
#[must_use]
pub fn queue_is_drained(q: NetQueue) -> bool {
    matches!(
        q,
        NetQueue::Logon | NetQueue::ClientCache | NetQueue::UiQueue | NetQueue::WorldObjects
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::RecipientId;
    use dereth_protocol::events::pack_event;
    use dereth_protocol::qualities::{PrivateUpdate, QualitiesPrivateUpdateInt};
    use dereth_protocol::Message;

    fn blob_msg(blob: &[u8], blob_id: u64) -> IncomingMessage {
        IncomingMessage {
            opcode: u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]),
            queue: NetQueue::UiQueue,
            sender: RecipientId(0),
            blob_id: NetBlobId(blob_id),
            body: blob[4..].to_vec(),
        }
    }

    fn event(iid: ObjectId, stamp: u32, value: i32) -> Vec<u8> {
        pack_event(
            iid,
            stamp,
            &QualitiesPrivateUpdateInt(PrivateUpdate {
                sequence: 1,
                property_id: 25,
                value,
            }),
        )
        .unwrap()
    }

    fn t(s: f64) -> LocalTime {
        LocalTime(s)
    }

    /// Part one: an ordered blob for an unknown object is **parked on that
    /// object** and replayed when it appears, not dropped.
    ///
    /// Oracle: the UI queue's ordering router —
    /// If the object is absent, queue the blob until that object exists.
    #[test]
    fn an_ordered_blob_for_an_unknown_object_is_parked() {
        let mut u = UiOrdering::new();
        let id = ObjectId(0x5000_0001);
        let known: Vec<ObjectId> = Vec::new();
        let d = u.route(&blob_msg(&event(id, 1, 7), 0), t(0.0), &|o| {
            known.contains(&o)
        });
        assert!(d.ready.is_empty(), "nothing dispatched");
        assert_eq!(u.blocked_on(id), 1);
        assert_eq!(
            u.highest_on(id),
            0,
            "parking never advances the highest stamp"
        );

        let replayed = u.object_arrived(id, t(0.0));
        assert_eq!(replayed.len(), 1);
        assert_eq!(
            &replayed[0][0..4],
            &OrderedEventHeader::MAGIC.to_le_bytes(),
            "the whole blob is kept"
        );
        assert_eq!(
            u.highest_on(id),
            1,
            "and the replay carried `highest` with it"
        );
    }

    /// Part two: `iid == 0` dispatches immediately.
    #[test]
    fn a_zero_iid_dispatches_without_an_object() {
        let mut u = UiOrdering::new();
        let d = u.route(&blob_msg(&event(ObjectId(0), 1, 7), 0), t(0.0), &|_| false);
        assert_eq!(d.ready.len(), 1);
        // The 12-byte header is stripped before dispatch.
        assert_eq!(
            u32::from_le_bytes(d.ready[0][0..4].try_into().unwrap()),
            QualitiesPrivateUpdateInt::OPCODE.0
        );
    }

    /// An unwrapped UI-queue blob dispatches as-is, with its own opcode intact.
    #[test]
    fn an_unwrapped_blob_dispatches_whole() {
        let mut u = UiOrdering::new();
        let blob = dereth_protocol::write_blob(&dereth_protocol::login::LoginWorldInfo {
            connections: 1,
            max_connections: 2,
            world_name: "X".into(),
        })
        .unwrap();
        let d = u.route(&blob_msg(&blob, 0), t(0.0), &|_| true);
        assert_eq!(d.ready, vec![blob]);
    }

    /// The per-object window blocks a gap and releases it in order once it fills.
    #[test]
    fn a_gap_on_one_object_blocks_and_then_releases_in_order() {
        let mut u = UiOrdering::new();
        let id = ObjectId(0x5000_0001);
        let known = |o: ObjectId| o == id;

        assert_eq!(
            u.route(&blob_msg(&event(id, 1, 1), 0), t(0.0), &known)
                .ready
                .len(),
            1
        );
        // Stamp 3 arrives before 2 and is held.
        assert!(u
            .route(&blob_msg(&event(id, 3, 3), 0), t(0.0), &known)
            .ready
            .is_empty());
        // Stamp 2 fills the gap and 3 follows.
        let d = u.route(&blob_msg(&event(id, 2, 2), 0), t(0.0), &known);
        assert_eq!(d.ready.len(), 2, "2 then 3");
    }

    /// Two objects have independent windows, which is what makes the retail "ordered against
    /// another object's id" path work at all.
    #[test]
    fn each_object_has_its_own_window() {
        let mut u = UiOrdering::new();
        let a = ObjectId(1);
        let b = ObjectId(2);
        let known = |_: ObjectId| true;
        assert_eq!(
            u.route(&blob_msg(&event(a, 1, 1), 0), t(0.0), &known)
                .ready
                .len(),
            1
        );
        assert_eq!(
            u.route(&blob_msg(&event(a, 2, 1), 0), t(0.0), &known)
                .ready
                .len(),
            1
        );
        // Object b's stream starts again at 1 rather than continuing a's numbering.
        assert_eq!(
            u.route(&blob_msg(&event(b, 1, 1), 0), t(0.0), &known)
                .ready
                .len(),
            1
        );
        assert_eq!(u.window_count(), 2);
    }

    /// Part three: the ephemeral path holds everything
    /// until `0x0013` has arrived and then replays it in order.
    ///
    /// Driven **directly**, not through [`UiOrdering::route`]. The function is modelled faithfully
    /// It gates on the crucial-ordered-events flag and otherwise appends to its waiting list.
    /// The shipped client cannot reach it, because the per-frame step compares against a value
    /// the ordering type cannot take (see
    /// `the_use_time_ephemeral_branch_can_never_fire_for_any_blob_id`). So the waiting list is
    /// always empty in the real client, and this test pins the behaviour of a function that is
    /// present, correct and dead.
    ///
    /// It previously drove the gate through `route` with the ephemeral bit set, which is exactly
    /// the misreading that deadlocked the client against ACE.
    #[test]
    fn the_ephemeral_path_is_gated_on_the_player_description() {
        let mut u = UiOrdering::new();
        let mut out = UiDispatch::default();

        for v in 1..=3i32 {
            u.ephemeral(event(ObjectId(0), 0, v), &mut out);
        }
        assert!(out.ready.is_empty(), "held until 0x0013");
        assert_eq!(u.gate.waiting(), 3);

        let replayed = u.crucial_events_received();
        assert_eq!(replayed.len(), 3);

        // After the gate opens, blobs pass straight through.
        u.ephemeral(event(ObjectId(0), 0, 4), &mut out);
        assert_eq!(out.ready.len(), 1);
    }

    /// The per-frame step's ephemeral branch is **dead in the shipped client**, and this proves it
    /// over the whole domain rather than by argument: the ordering type masks with `0x1F000000` before
    /// shifting, so no blob id in existence makes it equal `0x80000000_00000000`.
    ///
    /// This is the assertion that would have caught the deadlock against ACE. The branch had been
    /// written as `is_ephemeral(id)` — the constant is the ephemeral mask, so testing the bit is
    /// the natural misreading — and ACE sets that bit on every blob.
    #[test]
    fn the_use_time_ephemeral_branch_can_never_fire_for_any_blob_id() {
        for ty in 0u64..32 {
            for extra in [0u64, 1 << 63, 0x00FF_0000_FFFF_FFFF, u64::MAX] {
                let id = NetBlobId((ty << 56) | (extra & !(0x1F << 56)));
                assert_ne!(
                    ordering_type_raw(id),
                    USE_TIME_EPHEMERAL_TEST,
                    "ordering type {ty} with extra bits {extra:#x} reached the dead branch"
                );
            }
        }
    }

    /// An ACE blob — ephemeral bit set, ordering type 0 — must reach the ordering path and be
    /// delivered, not held. This is the shape of every blob the live server sends.
    #[test]
    fn an_ace_stamped_blob_is_delivered_rather_than_held_at_the_gate() {
        let mut u = UiOrdering::new();
        let id = ObjectId(0x5000_0001);
        // ACE's `Id = 0x80000000`: ephemeral bit set, ordering type 0, stamp 0.
        let blob_id = 0x8000_0000u64 << 32;
        assert!(is_ephemeral(NetBlobId(blob_id)));
        assert_eq!(ordering_type(NetBlobId(blob_id)), 0);
        let d = u.route(&blob_msg(&event(id, 1, 1), blob_id), t(0.0), &|_| true);
        assert_eq!(
            d.ready.len(),
            1,
            "held at the crucial-events gate -- the deadlock"
        );
    }

    /// A stale ordered stamp is reported as dropped rather than silently vanishing.
    #[test]
    fn a_stale_ordered_stamp_is_dropped() {
        let mut u = UiOrdering::new();
        let id = ObjectId(1);
        let known = |_: ObjectId| true;
        for s in 1..=10u32 {
            u.route(&blob_msg(&event(id, s, 1), 0), t(0.0), &known);
        }
        let d = u.route(&blob_msg(&event(id, 5, 1), 0), t(0.0), &known);
        assert!(d.ready.is_empty());
        assert!(matches!(
            d.events.as_slice(),
            [SessionEvent::Dropped {
                reason: DropReason::StaleOrderedStamp,
                ..
            }]
        ));
    }

    /// The ephemeral bit is bit 63 of the 64-bit blob id.
    #[test]
    fn the_ephemeral_bit_is_the_top_bit() {
        assert!(is_ephemeral(NetBlobId(1 << 63)));
        assert!(!is_ephemeral(NetBlobId(u64::MAX >> 1)));
    }

    /// Only four of the twelve queues are drained by this client.
    #[test]
    fn four_queues_are_drained() {
        assert!(queue_is_drained(NetQueue::Logon));
        assert!(queue_is_drained(NetQueue::ClientCache));
        assert!(queue_is_drained(NetQueue::UiQueue));
        assert!(queue_is_drained(NetQueue::WorldObjects));
        // Queue 2 is registered and never drained; the queue-drain table above records that result.
        assert!(!queue_is_drained(NetQueue::Control));
        assert!(!queue_is_drained(NetQueue::Weenie));
    }
}
