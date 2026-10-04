//! The single-request rule, every UI inventory-attempt sender and the complete error table.
//!
//! **The lock has no timeout, and that is the specification.** There is exactly one inventory
//! request in flight *globally* — not per object. If the server never answers, the retail client
//! wedges. The previous request's time is recorded and never read. Reproducing the wedge is
//! required; adding a timeout is a deliberate fix and belongs in the deviation ledger, not in the
//! port.
//!
//! **The lock is cleared in FOUR places, not two.** Retail has four places that clear the
//! previous request's object id:
//!
//! | clearer | when |
//! |---|---|
//! | the weenie object's record-response helper | when `id` is the lock's object, called by the ground-container arm |
//! | the stack-size response | same test, inlined — the **split**'s release |
//! | the move response | same test — the move's release |
//! | the attempt-failed response | the same test — see below |
//!
//! The attempt-failed response carries the same guard as the other three — it compares the
//! argument against the lock's object id and skips the stores when they differ.
//! [`RequestLock::clear`] below is nonetheless **observationally equivalent** to retail, for a
//! reason that belongs to a different function: the server-move handler's `0x00A0` arm prefers
//! the client's own lock id over the id in the message (it loads the lock's object id, tests it,
//! and moves it over the message's id when non-zero), so whenever a request is in flight the
//! object the call runs on **is** the lock's object and the guard passes; it can only fail when
//! the lock is already idle, where clearing changes nothing. Kept as an unconditional clear with
//! that reasoning written down, rather than silently relying on it.
//!
//! That is an argument about the **caller**: the `0x00A0` arm must perform the substitution, or
//! the two errors cancel for the lock and do not cancel for the object the handler runs on. The
//! arm calls [`RequestLock::substitute`], which is that substitution and nothing else.
//!
//! The stack-size clearer is not academic. A split's lock names the **source** stack while the
//! `0x0022` that comes back names the **new** object, so the move response's test cannot match:
//! without the stack-size release, one split wedges the client for the rest of the session and
//! every later inventory request is refused with [`BUSY_MESSAGE`].

use crate::weenie::{NameType, Weenie};
use dereth_primitives::{ObjectId, ServerTime};

/// Display local string information on channel `0x1A`, the channel every inventory refusal in
/// this crate goes out on.
pub const FEEDBACK_CHANNEL: u32 = 0x1A;

/// `InventoryRequest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum InventoryRequest {
    #[default]
    None = 0,
    Merge = 1,
    Split = 2,
    Move = 3,
    PickUp = 4,
    PutInContainer = 5,
    Drop = 6,
    Wield = 7,
    ViewAsGroundContainer = 8,
    Give = 9,
    ShopEvent = 10,
}

/// The three previous-request globals used for response correlation.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RequestLock {
    pub pending: InventoryRequest,
    pub object: Option<ObjectId>,
    /// The previous request's time, a `double` in the client's globals. Recorded and **read by
    /// nothing at all**.
    ///
    /// Not even the plugin API reads it: every access to either half is a store, and nothing
    /// reads it by any route. There is no reader to rebuild for. Kept so
    /// that the three variables are three variables, and so that nothing is tempted to time out on
    /// it.
    pub at: ServerTime,
}

impl RequestLock {
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.pending == InventoryRequest::None
    }

    /// Record the requested object id and request type.
    pub fn record(&mut self, object: ObjectId, kind: InventoryRequest, now: ServerTime) {
        self.pending = kind;
        self.object = Some(object);
        self.at = now;
    }

    /// The server-move handler's `0x00A0` arm: **the object the handler runs on is
    /// the client's own lock object id whenever the lock is held**, not the id in the
    /// message.
    ///
    /// ```text
    ///   id     = the message's object id
    ///   reason = the message's reason            ; NOT substituted
    ///   if (lock.object != 0) id = lock.object   ; <- the substitution
    ///   look up the requested world object by id
    /// ```
    ///
    /// Both the lock side and the arm end to end are covered by tests.
    ///
    /// The test is a zero test on the **id**, so a lock recorded against `ObjectId(0)` is not
    /// a lock for this purpose; [`RequestLock::clear`] stores `None`, which is the same state.
    ///
    /// This is the whole reason [`RequestLock::clear`] below is allowed to be unconditional: the
    /// only caller of the attempt-failed response in the entire client is the call just after
    /// this substitution, so the guard inside it can only ever
    /// be reached with `id == lock.object` or with an idle lock.
    #[must_use]
    pub fn substitute(&self, from_message: ObjectId) -> ObjectId {
        match self.object {
            Some(held) if held.0 != 0 => held,
            _ => from_message,
        }
    }

    /// Record a response for `id` — the whole operation:
    /// `if (id == lock.object) { lock.object = 0; lock.pending = IR_NONE; lock.at = 0; }`.
    ///
    /// The move response's clearing path, which only fires when the moved object matches;
    /// and the vendor UI's open-vendor guarded call,
    /// which is the **only** thing in retail that releases a shop-event request. See
    /// `crate::vendor`'s `handle_vendor_info` for the steps around it.
    pub fn clear_if_matches(&mut self, id: ObjectId) -> bool {
        if self.object == Some(id) {
            self.clear();
            true
        } else {
            false
        }
    }

    /// The attempt-failed response's clearing path.
    ///
    /// Retail guards this one too (it compares the argument against the lock's object id); it is
    /// unconditional here because the `0x00A0` arm hands the call the lock's object itself
    /// whenever the lock is held, so the guard can only fail on an already-idle lock. See the
    /// module header.
    ///
    /// **That equivalence is tested, not assumed.** The substitution in the `0x00A0` arm
    /// is modelled, the attempt-failed response has **exactly one caller in retail and no other
    /// route**, and the case the guard exists to reject is
    /// driven through both versions and shown to be unreachable *in retail*.
    ///
    /// The `0x00A0` arm must call [`RequestLock::substitute`] above rather than forward the
    /// message's own object id; that way this build reaches the guard on the same argument
    /// retail does, and the equivalence holds on both sides.
    pub fn clear(&mut self) {
        self.pending = InventoryRequest::None;
        self.object = None;
        self.at = ServerTime(0.0);
    }
}

/// The two refusal strings, verbatim.
pub const BUSY_MESSAGE: &str = "You can only move or use one item at a time";
pub const ATTACKING_MESSAGE: &str = "You cannot move or use an item while attacking";

/// Check whether the player is ready to make an inventory request.
///
/// Returns the client's own refusal string when it refuses. `quiet` suppresses the *message*, not
/// the refusal — the caller still gets `Err`, and only displays it when `quiet` is false.
///
/// # Errors
/// The refusal text, when a request is already in flight or an attack is in progress.
pub fn ready_for_inventory_request(
    lock: &RequestLock,
    attack_in_progress: bool,
) -> Result<(), &'static str> {
    if !lock.is_idle() {
        return Err(BUSY_MESSAGE);
    }
    if attack_in_progress {
        return Err(ATTACKING_MESSAGE);
    }
    Ok(())
}

/// Failure reasons reported by the server for inventory attempts.
///
/// Anything not listed produces the bare object-name message.
#[must_use]
pub fn attempt_failed_suffix(reason: u32) -> &'static str {
    match reason {
        0x1D => " - you're too busy",
        0x20 => " - you must control both objects",
        0x28 => " - the item is under someone else's control",
        0x2A => " - you are too encumbered",
        0x36 => " - action cancelled",
        0x37 => " - unable to move to object",
        0x3EE => " - the container is closed",
        _ => "",
    }
}

/// The attempt-failed handler's **name line**, one `sprintf` format per pending request.
///
/// **Taken from retail**; [`attempt_failed_suffix`] is the other half. The arms are
/// selected by the pending request 1..=9, anything else taking the default, and each has its own
/// format:
///
/// | pending [`InventoryRequest`] | literal |
/// |---|---|
/// | `Merge` 1 | `The %s can't be merged` |
/// | `Split` 2 | `The %s can't be split` |
/// | `Move` 3 | `The %s can't be moved` |
/// | `PickUp` 4 | `The %s can't be picked up` |
/// | `PutInContainer` 5 | `The %s can't be put in the container` |
/// | `Drop` 6 | `The %s can't be dropped` |
/// | `Wield` 7 | `The %s can't be wielded` |
/// | `Give` 9 | `The %s can't be given` |
///
/// **Slot 8 is not a gap in the table, it is `ViewAsGroundContainer` pointing at the
/// `default:` label** — the table's eighth entry is the `default:` target, which is where the
/// range check sends everything out of range. So that request and `None` draw **no name line
/// at all** and the whole message is the suffix, which is why a server-only replay (where the
/// pending request is `None` throughout) produces empty lines.
///
/// `ShopEvent` (10) is likewise out of the table's range and takes the same `default:`.
#[must_use]
pub fn attempt_failed_format(pending: InventoryRequest) -> Option<&'static str> {
    Some(match pending {
        InventoryRequest::Merge => "The %s can't be merged",
        InventoryRequest::Split => "The %s can't be split",
        InventoryRequest::Move => "The %s can't be moved",
        InventoryRequest::PickUp => "The %s can't be picked up",
        InventoryRequest::PutInContainer => "The %s can't be put in the container",
        InventoryRequest::Drop => "The %s can't be dropped",
        InventoryRequest::Wield => "The %s can't be wielded",
        InventoryRequest::Give => "The %s can't be given",
        InventoryRequest::None
        | InventoryRequest::ViewAsGroundContainer
        | InventoryRequest::ShopEvent => return None,
    })
}

/// The whole failure line: [`attempt_failed_format`] filled with the object name in the form the
/// arm asks for, then the suffix.
///
/// `IR_MERGE` and `IR_SPLIT` use the **plural** name (name type 1); every other
/// arm passes `2`, the *appropriate* form (plural iff the stack size
/// is above one).
///
/// The order is retail's: `sprintf(&acc, fmt, name)` **overwrites** the accumulator at
/// each arm's call site, and the suffix is appended to it afterwards. An arm with no
/// format therefore leaves the accumulator as the null
/// string and the line is the suffix alone.
#[must_use]
pub fn attempt_failed_text(
    pending: InventoryRequest,
    object: Option<&Weenie>,
    material_name: Option<&str>,
    reason: u32,
) -> String {
    let name_kind = match pending {
        InventoryRequest::Merge | InventoryRequest::Split => NameType::Plural,
        _ => NameType::Appropriate,
    };
    let name = object.map_or_else(String::new, |w| w.display_name(name_kind, material_name));
    let head =
        attempt_failed_format(pending).map_or_else(String::new, |fmt| fmt.replacen("%s", &name, 1));
    format!("{head}{}", attempt_failed_suffix(reason))
}

/// The refusal strings from the inventory error table that this crate produces itself.
pub mod messages {
    pub const ITEM_NOT_VALID: &str = "That item is not valid!";
    pub const MERGE_WHILE_TRADING: &str = "You cannot merge items while they are being traded.";
    pub const MERGE_DIFFERENT_TYPES: &str = "You cannot merge different types of items.";
    pub const DESTINATION_STACK_FULL: &str = "The destination stack is already full.";
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_protocol::types::PublicWeenieDesc;

    fn item(name: &str, stack: u16) -> Weenie {
        let mut w = Weenie::new(ObjectId(1));
        w.pwd = PublicWeenieDesc {
            name: name.into(),
            stack_size: Some(stack),
            ..PublicWeenieDesc::default()
        };
        w
    }

    /// Oracle: the recovered inventory behavior §6 —
    /// Uses the two exact refusal strings.
    #[test]
    fn the_lock_refuses_a_second_request_with_the_exact_string() {
        let mut lock = RequestLock::default();
        assert_eq!(ready_for_inventory_request(&lock, false), Ok(()));

        lock.record(ObjectId(7), InventoryRequest::Wield, ServerTime(1.0));
        assert_eq!(
            ready_for_inventory_request(&lock, false),
            Err("You can only move or use one item at a time")
        );

        lock.clear();
        assert_eq!(
            ready_for_inventory_request(&lock, true),
            Err("You cannot move or use an item while attacking")
        );
    }

    /// The lock has no timeout and a mismatched reply does not clear it.
    #[test]
    fn the_lock_has_no_timeout_and_a_mismatched_reply_does_not_clear_it() {
        let mut lock = RequestLock::default();
        lock.record(ObjectId(7), InventoryRequest::Wield, ServerTime(1.0));

        assert!(
            !lock.clear_if_matches(ObjectId(8)),
            "an unrelated object does not clear it"
        );
        assert!(!lock.is_idle());

        // A million seconds later the lock is still held: the previous request time is never read.
        assert!(
            ready_for_inventory_request(&lock, false).is_err(),
            "the retail client wedges permanently; do not add a timeout"
        );

        assert!(lock.clear_if_matches(ObjectId(7)));
        assert!(lock.is_idle());
    }

    /// The failure line uses the documented suffixes and name form.
    #[test]
    fn the_failure_line_uses_the_documented_suffixes_and_name_form() {
        let w = item("Pyreal", 5);
        assert_eq!(
            attempt_failed_text(InventoryRequest::Merge, Some(&w), None, 0x2A),
            "The Pyreals can't be merged - you are too encumbered",
            "IR_MERGE uses the plural form"
        );
        assert_eq!(
            attempt_failed_text(InventoryRequest::Wield, Some(&item("Sword", 1)), None, 0x1D),
            "The Sword can't be wielded - you're too busy",
            "everything else uses the appropriate form"
        );
        assert_eq!(
            attempt_failed_text(InventoryRequest::Drop, Some(&item("Sword", 1)), None, 0x999),
            "The Sword can't be dropped",
            "an unlisted reason adds no suffix -- the format alone is the message"
        );
        for (code, suffix) in [
            (0x1Du32, " - you're too busy"),
            (0x20, " - you must control both objects"),
            (0x28, " - the item is under someone else's control"),
            (0x2A, " - you are too encumbered"),
            (0x36, " - action cancelled"),
            (0x37, " - unable to move to object"),
            (0x3EE, " - the container is closed"),
        ] {
            assert_eq!(attempt_failed_suffix(code), suffix, "reason 0x{code:X}");
        }
    }

    /// Every arm of the jump table has its own format.
    #[test]
    fn every_arm_of_the_jump_table_has_its_own_format() {
        use InventoryRequest as R;
        for (r, f) in [
            (R::Merge, Some("The %s can't be merged")),
            (R::Split, Some("The %s can't be split")),
            (R::Move, Some("The %s can't be moved")),
            (R::PickUp, Some("The %s can't be picked up")),
            (
                R::PutInContainer,
                Some("The %s can't be put in the container"),
            ),
            (R::Drop, Some("The %s can't be dropped")),
            (R::Wield, Some("The %s can't be wielded")),
            (R::Give, Some("The %s can't be given")),
            (R::None, None),
            (R::ViewAsGroundContainer, None),
            (R::ShopEvent, None),
        ] {
            assert_eq!(attempt_failed_format(r), f, "{r:?}");
        }
        assert_eq!(
            attempt_failed_text(
                InventoryRequest::ViewAsGroundContainer,
                Some(&item("Chest", 1)),
                None,
                0x1D,
            ),
            " - you're too busy",
            "the default: arm leaves the accumulator empty and the suffix is the whole line"
        );
    }

    /// Oracle: §6's `InventoryRequest` enum.
    #[test]
    fn the_request_enum_has_the_documented_ordinals() {
        assert_eq!(InventoryRequest::None as u32, 0);
        assert_eq!(InventoryRequest::Merge as u32, 1);
        assert_eq!(InventoryRequest::Split as u32, 2);
        assert_eq!(InventoryRequest::Move as u32, 3);
        assert_eq!(InventoryRequest::PickUp as u32, 4);
        assert_eq!(InventoryRequest::PutInContainer as u32, 5);
        assert_eq!(InventoryRequest::Drop as u32, 6);
        assert_eq!(InventoryRequest::Wield as u32, 7);
        assert_eq!(InventoryRequest::ViewAsGroundContainer as u32, 8);
        assert_eq!(InventoryRequest::Give as u32, 9);
        assert_eq!(InventoryRequest::ShopEvent as u32, 10);
    }
}
