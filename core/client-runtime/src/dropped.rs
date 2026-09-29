//! The dropped-opcode ledger.
//!
//! # Why this module exists
//!
//! Inbound opcodes that no receiver handles are otherwise dropped silently. The two places the
//! drop happens are `ObjectStream::world_view`'s `_ => self.stats.unhandled += 1`
//! (`objects.rs`) and `Hud::ui_event`'s `_ => {}` (`hud.rs`), and
//!
//! * `ObjectStream::stats.unhandled` is a bare count with no production reader; and
//! * `Hud::ui_event` does not even count. It returns.
//!
//! This module is the instrument that says which opcodes went unreceived, read off a running
//! client instead of found by hand. It is deliberately *not* a counter on either struct: a `u64`
//! says how many were dropped and never **which**, and "which" is the entire question.
//!
//! # What a human sees
//!
//! The **first** time an opcode is shown to reach no receiver, one line goes to the client's log
//! (a `warn` event) naming the opcode, its catalogue name, the site and the count so far. Repeats are counted silently, so a
//! `0xF7B0`-wrapped message that arrives 39 times in a session costs one line, not 39.
//!
//! The site matters because it is the one thing the ledger knows and an opcode table cannot:
//! `world_view` and `ui_event` are different routers and an opcode at the wrong one is a routing
//! bug, not a missing handler. The line is also readable back inside the process
//! (`announcements`), because "one line per opcode per session, and not a second one" is a
//! claim about the log that no test could otherwise make without installing a subscriber.
//!
//! # Per thread, like `dereth_ui_screens::requests`
//!
//! The frame loop is single-threaded, as the client's is, so a thread-local needs no lock and —
//! more usefully — one test's drops cannot be seen by another running beside it.
//!
//! # This is an instrument, not a policy
//!
//! Recording a drop does **not** mean the opcode should get a receiver: many opcodes a static
//! listing would call unhandled never arrive in the recorded retail sessions at all, and some
//! (`0xF7E2`, `0xF7E4`, `0xF7E5`, `0xF7E7`) are never dropped at all.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use dereth_protocol::Opcode;

/// Which wildcard arm ate the message. The two are not interchangeable: a message that reaches
/// [`Site::WorldObjects`] took the world-object path and one that reaches [`Site::UiEvent`] took the
/// UI queue, and an opcode appearing at the wrong one is a routing bug rather than a missing
/// receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Site {
    /// `ObjectStream::world_view`'s final arm — the world-message dispatch switch.
    WorldObjects,
    /// `Hud::ui_event`'s final arm — the UI queue's dispatch.
    UiEvent,
    /// `interaction::apply_events_at_boundary`'s final arm. It sees the **same**
    /// `SessionEvent::UiEvent` list `Hud::ui_event` does, so a message recorded here is only
    /// genuinely unreceived when it is recorded at [`Site::UiEvent`] as well.
    Interaction,
}

impl Site {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorldObjects => "smartbox",
            Self::UiEvent => "ui_event",
            Self::Interaction => "interaction",
        }
    }
}

/// One row of the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drop {
    pub site: Site,
    pub opcode: Opcode,
    pub count: u64,
}

thread_local! {
    static LEDGER: RefCell<BTreeMap<(Site, u32), u64>> = const { RefCell::new(BTreeMap::new()) };
    /// Opcodes already named in the log, so a message that arrives 39 times costs one line.
    static ANNOUNCED: RefCell<BTreeSet<u32>> = const { RefCell::new(BTreeSet::new()) };
    /// Every line [`record`] has put in the log this session, verbatim.
    ///
    /// The line is the deliverable and a log event cannot be read back, so a test that wants to
    /// assert *"one line, and not a second one"* has nothing to point at. This is that
    /// instrument's own instrument: it holds the exact string that was printed, so
    /// `announcements` and the client's log carry the same text by construction rather than by
    /// two format strings that agree today. It is bounded by the number of distinct opcodes the
    /// session drops — one `String` per opcode, never per message.
    static ANNOUNCEMENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Count one dropped message and, the first time the opcode is shown to have **no receiver at
/// all**, say so in the log. Returns the running count for the `(site, opcode)` pair.
///
/// "No receiver at all" is not the same as "this arm dropped it", and conflating the two is what
/// would make this instrument useless. [`Site::UiEvent`] and [`Site::Interaction`] are handed the
/// *same* `SessionEvent::UiEvent` list, so the ~100 opcodes one of them consumes all fall through
/// the other's wildcard on every single message; announcing per site would bury the real finding
/// under a hundred lines of noise on the first login. An opcode is announced only when **both** UI
/// sites have refused it. [`Site::WorldObjects`] has no second consumer, so its refusal is final on
/// its own.
///
/// # What the line says
///
/// ```text
/// inbound 0x0004 Communication_PopUpString reached no receiver (ui_event, 1 so far)
/// ```
///
/// Four facts, because each answers a different question a person
/// reading a log actually has: **the opcode** (which message), **its catalogue name** from
/// `dereth_protocol::opcodes` (what message — the number alone sends the reader to a table), **the
/// site** (`world_view` / `ui_event` / `interaction`, i.e. *which* router refused it, which is the
/// difference between a routing bug and a missing receiver — see [`Site`]), and **the count so
/// far at that site** (almost always 1, and interesting exactly when it is not: a burst that
/// arrived before the second UI consumer had its turn).
///
/// It goes to **the log and nowhere else**. Retail has no such line at all — it has a handler for
/// every opcode — so this is an instrument, not a transcription, and putting it in the chat window
/// would be inventing player-visible text the client never had.
pub fn record(site: Site, opcode: Opcode) -> u64 {
    let n = LEDGER.with(|l| {
        let mut l = l.borrow_mut();
        let slot = l.entry((site, opcode.0)).or_insert(0);
        *slot += 1;
        *slot
    });
    if unreceived(opcode) {
        ANNOUNCED.with(|a| {
            if a.borrow_mut().insert(opcode.0) {
                let line = format!(
                    "inbound {:#06X} {} reached no receiver ({}, {n} so far)",
                    opcode.0,
                    opcode.name().unwrap_or("not in the opcode table"),
                    site.as_str(),
                );
                tracing::warn!("{line}");
                ANNOUNCEMENTS.with(|v| v.borrow_mut().push(line));
            }
        });
    }
    n
}

/// Every line [`record`] has printed this session, in the order it printed them.
///
/// This is the same `String` that went to the log, not a reconstruction: see [`ANNOUNCEMENTS`].
#[must_use]
pub fn announcements() -> Vec<String> {
    ANNOUNCEMENTS.with(|v| v.borrow().clone())
}

/// Whether the ledger has seen enough to say this opcode reaches **no** receiver: the WorldObjects arm
/// refused it, or both UI-queue arms did.
#[must_use]
pub fn unreceived(opcode: Opcode) -> bool {
    count(Site::WorldObjects, opcode) > 0
        || (count(Site::UiEvent, opcode) > 0 && count(Site::Interaction, opcode) > 0)
}

/// Every `(site, opcode)` pair dropped so far on this thread, in `(site, opcode)` order.
#[must_use]
pub fn snapshot() -> Vec<Drop> {
    LEDGER.with(|l| {
        l.borrow()
            .iter()
            .map(|(&(site, opcode), &count)| Drop {
                site,
                opcode: Opcode(opcode),
                count,
            })
            .collect()
    })
}

/// How many times `opcode` has been dropped at `site`, zero if never.
#[must_use]
pub fn count(site: Site, opcode: Opcode) -> u64 {
    LEDGER.with(|l| l.borrow().get(&(site, opcode.0)).copied().unwrap_or(0))
}

/// Every message dropped so far, at either site.
#[must_use]
pub fn total() -> u64 {
    LEDGER.with(|l| l.borrow().values().sum())
}

/// Empty the ledger. Tests call this at their start; production never does — the ledger is a
/// session-long record and a log-off does not make a dropped message un-dropped.
pub fn clear() {
    LEDGER.with(|l| l.borrow_mut().clear());
    ANNOUNCED.with(|a| a.borrow_mut().clear());
    ANNOUNCEMENTS.with(|v| v.borrow_mut().clear());
}
