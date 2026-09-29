//! Quest definitions, contracts and the tracker's **display-time** duration computation.
//!
//! There is no client-side quest engine: a quest is a named flag on the character held entirely on
//! the server, and the client never sees the flag table. What is here is the contract *tracker*.
//!
//! **The near-miss with the enchantment rebasing**: a contract's remaining time
//! is `time_when_repeats − (now − time_of_server_update)`, computed **at display time**, where an
//! enchantment's is rebased onto the clock at *receipt*. Do not copy the rebasing here.

use dereth_primitives::ServerTime;
use dereth_protocol::social::{
    SocialSendClientContractTracker, SocialSendClientContractTrackerTable,
};
use std::collections::BTreeMap;

use crate::{Request, RequestSink, World};

/// One quest definition. The original `QuestDef` layout occupies 16 bytes; this Rust type models
/// the decoded values rather than that physical layout.
///
/// The client never evaluates `min_delta` or `max_solves` — no reader was found in this build; the
/// table is loaded and kept so a UI or a plugin can render "you may repeat this in N".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestDef {
    /// Stored **de-obfuscated**: swaps each byte's nibbles over
    /// the length-minus-one bytes, exactly as spell and component names are.
    pub full_name: String,
    /// Seconds before the quest may be solved again.
    pub min_delta: i32,
    /// `-1` = unlimited.
    pub max_solves: i32,
}

impl Default for QuestDef {
    /// The constructor's defaults: `min_delta = 0`, `max_solves = -1`.
    fn default() -> Self {
        Self {
            full_name: String::new(),
            min_delta: 0,
            max_solves: -1,
        }
    }
}

/// The nibble swap shared by quest, spell and component names.
///
/// `c = (c >> 4) | (c << 4)` over `len − 1` bytes — the **last** byte is left alone.
#[must_use]
pub fn nibble_swap(bytes: &[u8]) -> Vec<u8> {
    let n = bytes.len().saturating_sub(1);
    bytes
        .iter()
        .enumerate()
        .map(|(i, c)| if i < n { c.rotate_left(4) } else { *c })
        .collect()
}

/// The client's contract record (0x110) — the static description, a presentation wrapper around
/// up to six server-side quest flags plus three map locations.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Contract {
    pub version: u32,
    pub contract_id: u32,
    pub contract_name: String,
    pub description: String,
    /// A `printf`-style template taking the stage number.
    pub description_progress: String,
    pub name_npc_start: String,
    pub name_npc_end: String,
    pub questflag_stamped: String,
    pub questflag_started: String,
    pub questflag_finished: String,
    pub questflag_progress: String,
    pub questflag_timer: String,
    pub questflag_repeat_time: String,
}

/// The live contract-progress record; the original native record is `0x28` bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ContractTracker {
    pub version: u32,
    pub contract_id: u32,
    pub contract_stage: u32,
    pub time_when_done: f64,
    pub time_when_repeats: f64,
    /// **Not on the wire.** The client stamps it locally when the record arrives, so it can age the
    /// two server-supplied intervals.
    pub time_of_server_update: f64,
}

impl ContractTracker {
    /// Stamp `time_of_server_update` — what the client does on receipt.
    pub fn on_receipt(&mut self, now: ServerTime) {
        self.time_of_server_update = now.0;
    }
}

/// The four stages the contracts panel renders.
pub mod stage {
    pub const AVAILABLE: u32 = 1;
    pub const IN_PROGRESS: u32 = 2;
    pub const DONE: u32 = 3;
    /// Stage ≥ 4 is a numbered sub-stage: `sprintf(description_progress, stage − 4)`.
    pub const FIRST_SUBSTAGE: u32 = 4;
}

/// The progress formatter's stage-1 literal.
///
/// **The five literals below are the client's own.** Three of them differ from the obvious
/// reading; see
/// [`fill_progress_string`]. `"Available"` is pushed from **three** distinct sites.
pub const AVAILABLE: &str = "Available";
/// Stage 2, and the empty-`description_progress` arm of stage >= 4.
pub const IN_PROGRESS: &str = "In Progress";
/// Stage 3 with no repeat timer and no `questflag_repeat_time`.
pub const DONE: &str = "Done";
/// The client's answer when the location query refuses the cell.
///
/// **Not `"None"`.** The client says `"Indoors"`, which is also the only reading that makes sense
/// for a contract whose NPC stands in a building. Measured against retail.
pub const INDOORS: &str = "Indoors";
/// The timed-text value when the contract carries no `questflag_timer`.
pub const NO_TIMER: &str = "None";
/// The timed-text value when the timer has run out.
pub const TIMER_FINISHED: &str = "Finished";

/// Format contract progress for the contracts UI.
///
/// The stage-3 countdown is `time_when_repeats − (now − time_of_server_update)` — computed
/// **at display time**, not rebased on receipt.
///
/// `delta_time_to_string` is supplied by the display layer, which belongs to the UI; the
/// caller supplies it so this stays free of formatting policy.
#[must_use]
pub fn fill_progress_string(
    t: &ContractTracker,
    c: &Contract,
    now: ServerTime,
    delta_time_to_string: &dyn Fn(f64) -> String,
) -> String {
    match t.contract_stage {
        stage::AVAILABLE => AVAILABLE.to_string(),
        stage::IN_PROGRESS => IN_PROGRESS.to_string(),
        stage::DONE => {
            if t.time_when_repeats > 0.0 {
                let remaining = t.time_when_repeats - (now.0 - t.time_of_server_update);
                if remaining > 0.0 {
                    // The float-to-integer conversion truncates — the remaining time is
                    // **truncated to whole
                    // seconds** before `delta_time_to_string` ever sees it.
                    format!(
                        "Done ({} to Repeat)",
                        delta_time_to_string(remaining.trunc())
                    )
                } else {
                    // **`"Available"`, not `"Done"`.** The formatter pushes the same
                    // literal the stage-1 arm pushes — a repeat timer that has run out puts the
                    // contract back on offer. Verified against retail; `"Done"` is not what this
                    // arm pushes.
                    AVAILABLE.to_string()
                }
            } else if c.questflag_repeat_time.is_empty() {
                // A repeat-time string of length 1 (just the terminator), i.e. the empty string.
                DONE.to_string()
            } else {
                // **`"Available"`, not the empty string.** The formatter uses that literal again:
                // a repeatable contract with no timer running is available.
                AVAILABLE.to_string()
            }
        }
        s if s >= stage::FIRST_SUBSTAGE => {
            // an **empty** `description_progress` short-circuits to `"In Progress"`
            // rather than formatting an empty template.
            if c.description_progress.is_empty() {
                IN_PROGRESS.to_string()
            } else {
                // `sprintf(description_progress, stage - 4)`. The template's
                // single `%d` is the only conversion the shipped strings use.
                c.description_progress
                    .replacen("%d", &(s - stage::FIRST_SUBSTAGE).to_string(), 1)
            }
        }
        // Stage 0 — writes nothing at all, leaving the caller's empty string.
        _ => String::new(),
    }
}

/// The contracts panel's sort criteria.
pub use dereth_client_contract::panels::contracts::ContractSort;

/// The contract-tracker table owned by player state.
#[derive(Debug, Clone, Default)]
pub struct ContractTrackerTable {
    pub trackers: BTreeMap<u32, ContractTracker>,
}

impl ContractTrackerTable {
    /// Replace the whole contract-tracker table from message `0x0314`.
    pub fn replace(&mut self, trackers: Vec<ContractTracker>, now: ServerTime) {
        self.trackers.clear();
        for mut t in trackers {
            t.on_receipt(now);
            self.trackers.insert(t.contract_id, t);
        }
    }

    /// Update one contract tracker from message `0x0315`.
    pub fn update(&mut self, mut t: ContractTracker, now: ServerTime) {
        t.on_receipt(now);
        self.trackers.insert(t.contract_id, t);
    }

    /// Removes the tracker once the server confirms.
    pub fn remove(&mut self, contract_id: u32) -> bool {
        self.trackers.remove(&contract_id).is_some()
    }
}

/// The contracts UI location string, `"%.1f%s, %.1f%s"`, with the direction letters.
///
/// **Three things here are measured rather than transcribed, and all three are easy to get
/// wrong.**
///
/// 1. **The north/south value is printed first.** The client pushes the argument pairs in reverse,
///    so `sprintf` receives `(|ns|, ns_letter, |ew|, ew_letter)`.
/// 2. **Exactly zero prints no letter at all.** The literal table contains `L"N"`, `L"S"`,
///    `L"E"`, and `L"W"` plus the **empty** string, which both axis branches select when their
///    comparison against `0.0` says
///    equal. So a contract sitting on the origin line reads `"0.0, 12.3E"`.
/// 3. **The magnitude is `fabs`**, not a negation — which is the same thing for a
///    finite value and is named so the call site is not mistaken for a sign flip.
#[must_use]
pub fn location_string(ns: f64, ew: f64) -> String {
    let letter = |v: f64, pos: &'static str, neg: &'static str| {
        if v < 0.0 {
            neg
        } else if v > 0.0 {
            pos
        } else {
            ""
        }
    };
    let (nd, ed) = (letter(ns, "N", "S"), letter(ew, "E", "W"));
    format!("{:.1}{nd}, {:.1}{ed}", ns.abs(), ew.abs())
}

/// The client's whole answer for one of the three `Position`s.
///
/// ```text
///   objcell_id == 0                   -> the field is LEFT ALONE
///   convert the global id to land coordinates
///   conversion failed                 -> "Indoors"
/// ```
///
/// `None` is `gid_to_lcoord` refusing the cell — an interior cell id, or one out of bounds — and
/// the answer is [`INDOORS`]. A **zero** `objcell_id` is a different case that this function
/// cannot express, because the client writes nothing at all for it; the caller must not call in.
#[must_use]
pub fn contract_location_text(coords: Option<(f64, f64)>) -> String {
    match coords {
        Some((ns, ew)) => location_string(ns, ew),
        None => INDOORS.to_string(),
    }
}

/// Timed-text update.
///
/// ```text
/// if (questflag_timer is empty)                      "None"
/// else { remaining = time_when_done - (now - time_of_server_update)
///        if (remaining > 0.0)  delta_time_to_string((long)remaining)
///        else                  "Finished" }
/// ```
///
/// The `(long)` truncates, exactly as [`fill_progress_string`]'s repeat countdown
/// truncates — so the line steps by whole seconds.
#[must_use]
pub fn timed_string(
    t: &ContractTracker,
    c: &Contract,
    now: ServerTime,
    delta_time_to_string: &dyn Fn(f64) -> String,
) -> String {
    if c.questflag_timer.is_empty() {
        return NO_TIMER.to_string();
    }
    let remaining = t.time_when_done - (now.0 - t.time_of_server_update);
    if remaining > 0.0 {
        delta_time_to_string(remaining.trunc())
    } else {
        TIMER_FINISHED.to_string()
    }
}

/// Contract-contact selection.
///
/// ```text
/// if (name_npc_end is empty)                    -> start
/// else if (stage == 2 || stage >= 4)             -> end
/// else if (name_npc_start is empty)             -> end
/// else                                           -> start
/// ```
///
/// True means the **end** NPC and `location_npc_end`; false means the start NPC and
/// `location_npc_start`. The `stage < 4 && stage != 2` window is stages 0, 1 and 3, i.e. "not
/// yet taken, not under way, or finished" — the player is being told where to *begin*.
#[must_use]
pub fn contact_is_the_end_npc(c: &Contract, stage: u32) -> bool {
    if c.name_npc_end.is_empty() {
        return false;
    }
    if stage == stage::IN_PROGRESS || stage >= stage::FIRST_SUBSTAGE {
        return true;
    }
    c.name_npc_start.is_empty()
}

/// How a `0x0315` changed the table — returned so a caller can tell "one arrived and meant
/// nothing" from "none arrived", which an arrival counter alone cannot separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractUpdate {
    /// Contract-tracker insert arm.
    Added,
    /// Contract-tracker update-in-place arm.
    Updated,
    /// Contract-tracker remove arm.
    Removed,
}

impl World {
    /// Receive full contract-tracker table message `0x0314`.
    ///
    /// Empty the table, then one stamped `add` per record: the whole table is **replaced**, and
    /// every record's `time_of_server_update` becomes the current time. Returns how
    /// many trackers the table now holds.
    pub fn recv_contract_tracker_table(
        &mut self,
        m: &SocialSendClientContractTrackerTable,
        now: ServerTime,
    ) -> usize {
        let trackers: Vec<ContractTracker> =
            m.0.entries
                .iter()
                .map(|(_, t)| ContractTracker {
                    version: t.version,
                    contract_id: t.contract_id,
                    contract_stage: t.contract_stage,
                    time_when_done: t.time_when_done,
                    time_when_repeats: t.time_when_repeats,
                    time_of_server_update: 0.0,
                })
                .collect();
        self.contracts.replace(trackers, now);
        self.contracts.trackers.len()
    }

    /// Compose the social contract-tracker handler with the single-tracker update — the `0x0315`
    /// receiver.
    ///
    /// ```text
    ///   if (not a delete)            -> update or insert
    ///   if (delete, id not held)     -> INSERT it
    ///   if (delete, id held)         -> remove, then fall into the update arm
    ///   update arm: a held record is updated in place (stage, both doubles, cur_time stamp);
    ///               otherwise stamp cur_time and add
    /// ```
    ///
    /// Three readings that are retail's and not the obvious ones:
    ///
    /// * **A delete for an unknown contract inserts it.** The delete-miss branch jumps straight into the add
    ///   arm. ACE never sends that shape (ACE's contract erase only flags a row it holds), so
    ///   it has no live consequence, but the arm is here rather than silently swallowed.
    /// * **A delete that *does* find the record then writes through the freed pointer**
    ///   by re-testing the same pointer. Retail's use-after-free has one observable result: a
    ///   removal, which is what this does.
    /// * **`version` is not copied on an update.** The update arm writes stage, the two doubles
    ///   and the server-update stamp and nothing else, so a record's version is whichever arrival
    ///   created it.
    ///
    /// `set_as_display_contract` reaches the display-contract setter, but **this build has no
    /// consumer for the pinned contract**, so it is returned to the caller to count
    /// rather than stored in a field nothing reads.
    pub fn recv_contract_tracker(
        &mut self,
        m: &SocialSendClientContractTracker,
        now: ServerTime,
    ) -> ContractUpdate {
        let t = ContractTracker {
            version: m.tracker.version,
            contract_id: m.tracker.contract_id,
            contract_stage: m.tracker.contract_stage,
            time_when_done: m.tracker.time_when_done,
            time_when_repeats: m.tracker.time_when_repeats,
            time_of_server_update: 0.0,
        };
        let held = self.contracts.trackers.contains_key(&t.contract_id);
        if m.delete_contract != 0 {
            if held {
                self.contracts.remove(t.contract_id);
                return ContractUpdate::Removed;
            }
            // The remove path jumps into the add arm. See the note above.
            self.contracts.update(t, now);
            return ContractUpdate::Added;
        }
        if let Some(e) = self.contracts.trackers.get_mut(&t.contract_id) {
            // stage, `time_when_done`, `time_when_repeats`, then the stamp.
            e.contract_stage = t.contract_stage;
            e.time_when_done = t.time_when_done;
            e.time_when_repeats = t.time_when_repeats;
            e.on_receipt(now);
            return ContractUpdate::Updated;
        }
        self.contracts.update(t, now);
        ContractUpdate::Added
    }

    /// The live trackers, ordered by contract id — what the panel joins against the dat table.
    #[must_use]
    pub fn contract_trackers(&self) -> &BTreeMap<u32, ContractTracker> {
        &self.contracts.trackers
    }

    /// Send abandon-contract event `0x0316` with a four-byte contract id.
    ///
    /// The client's `0x100005DC` arm is the **only**
    /// caller in the client, and it refuses `contract_id == 0` before sending.
    ///
    /// **It does not remove the tracker.** The table changes when the shard answers with a
    /// `0x0315` carrying `delete_contract`, which is why [`ContractTrackerTable::remove`] is
    /// reached from the receive path and not from here.
    ///
    /// Returns `None` when the id was zero, i.e. when nothing was sent.
    pub fn abandon_contract(&mut self, req: &mut dyn RequestSink, contract_id: u32) -> Option<()> {
        if contract_id == 0 {
            return None;
        }
        req.send(Request::SocialAbandonContract(
            dereth_protocol::social::SocialAbandonContract { contract_id },
        ));
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(v: f64) -> String {
        format!("{v:.0}s")
    }

    /// Oracle: the recovered contract formatter and its near-miss case — durations are computed at
    /// **display** time.
    #[test]
    fn contract_durations_render_from_the_display_clock() {
        let c = Contract {
            questflag_repeat_time: "SomeFlag_Repeat".into(),
            description_progress: "Step %d of the hunt".into(),
            ..Contract::default()
        };
        let mut t = ContractTracker {
            contract_id: 7,
            contract_stage: stage::DONE,
            time_when_repeats: 100.0,
            ..ContractTracker::default()
        };
        // The record arrived at t = 1000.
        t.on_receipt(ServerTime(1000.0));

        // At the instant of arrival the full 100 s remains.
        assert_eq!(
            fill_progress_string(&t, &c, ServerTime(1000.0), &seconds),
            "Done (100s to Repeat)"
        );
        // 40 s later, 60 s remains — the *display* clock does the subtraction, so a late render
        // shows less, and re-rendering the same record at the same instant always agrees.
        assert_eq!(
            fill_progress_string(&t, &c, ServerTime(1040.0), &seconds),
            "Done (60s to Repeat)"
        );
        assert_eq!(
            fill_progress_string(&t, &c, ServerTime(1101.0), &seconds),
            "Available"
        );

        // A second copy of the same record received later shows the same remaining time at the
        // same instant, which is the whole difference from the enchantment path.
        let mut later = t;
        later.on_receipt(ServerTime(1040.0));
        assert_eq!(
            fill_progress_string(&later, &c, ServerTime(1040.0), &seconds),
            "Done (100s to Repeat)",
            "the record is aged from its own arrival, so a re-send restarts the countdown"
        );
    }

    /// Oracle: §2's stage table.
    #[test]
    fn the_four_stages_render_the_documented_strings() {
        let c = Contract {
            description_progress: "Step %d of the hunt".into(),
            ..Contract::default()
        };
        let mut t = ContractTracker {
            contract_id: 7,
            ..ContractTracker::default()
        };
        t.contract_stage = stage::AVAILABLE;
        assert_eq!(
            fill_progress_string(&t, &c, ServerTime(0.0), &seconds),
            "Available"
        );
        t.contract_stage = stage::IN_PROGRESS;
        assert_eq!(
            fill_progress_string(&t, &c, ServerTime(0.0), &seconds),
            "In Progress"
        );
        t.contract_stage = stage::DONE;
        assert_eq!(
            fill_progress_string(&t, &c, ServerTime(0.0), &seconds),
            "Done",
            "no repeat timer and no repeat flag: a bare Done"
        );
        let repeatable = Contract {
            questflag_repeat_time: "SomeFlag_Repeat".into(),
            ..c.clone()
        };
        assert_eq!(
            fill_progress_string(&t, &repeatable, ServerTime(0.0), &seconds),
            "Available"
        );
        t.contract_stage = 4;
        assert_eq!(
            fill_progress_string(&t, &c, ServerTime(0.0), &seconds),
            "Step 0 of the hunt"
        );
        t.contract_stage = 9;
        assert_eq!(
            fill_progress_string(&t, &c, ServerTime(0.0), &seconds),
            "Step 5 of the hunt"
        );
        // An empty `_description_progress` is `"In Progress"`, not an empty line.
        assert_eq!(
            fill_progress_string(&t, &Contract::default(), ServerTime(0.0), &seconds),
            "In Progress"
        );
        // Stage 0 writes nothing.
        t.contract_stage = 0;
        assert_eq!(fill_progress_string(&t, &c, ServerTime(0.0), &seconds), "");
    }

    /// Oracle: §1 — the nibble swap runs over `len − 1` bytes, leaving the **last** one alone. The
    /// same routine obfuscates spell and component names.
    #[test]
    fn the_name_obfuscation_leaves_the_last_byte_alone() {
        let raw = [0x12u8, 0x34, 0x56];
        assert_eq!(nibble_swap(&raw), vec![0x21, 0x43, 0x56]);
        assert_eq!(nibble_swap(&[]), Vec::<u8>::new());
        assert_eq!(
            nibble_swap(&[0xAB]),
            vec![0xAB],
            "a one-byte name is untouched"
        );
        // The swap is its own inverse on the bytes it touches.
        assert_eq!(nibble_swap(&nibble_swap(&raw)), raw.to_vec());
    }

    /// Oracle: §1 — the constructor defaults.
    #[test]
    fn quest_def_defaults_to_no_cooldown_and_unlimited_solves() {
        let d = QuestDef::default();
        assert_eq!(d.min_delta, 0);
        assert_eq!(d.max_solves, -1);
    }

    /// Locations render with direction letters and indoors.
    #[test]
    fn locations_render_with_direction_letters_and_indoors() {
        assert_eq!(location_string(23.4, -11.2), "23.4N, 11.2W");
        assert_eq!(location_string(-1.0, 2.0), "1.0S, 2.0E");
        // The neutral compass suffix is the empty string, selected by an exactly-zero axis.
        assert_eq!(location_string(0.0, 2.0), "0.0, 2.0E");
        assert_eq!(contract_location_text(Some((1.0, 2.0))), "1.0N, 2.0E");
        assert_eq!(contract_location_text(None), "Indoors");
    }

    /// The timed line is none then a countdown then finished.
    #[test]
    fn the_timed_line_is_none_then_a_countdown_then_finished() {
        let timed = Contract {
            questflag_timer: "SomeFlag_Timer".into(),
            ..Contract::default()
        };
        let mut t = ContractTracker {
            contract_id: 7,
            time_when_done: 90.5,
            ..ContractTracker::default()
        };
        t.on_receipt(ServerTime(1000.0));
        assert_eq!(
            timed_string(&t, &Contract::default(), ServerTime(1000.0), &seconds),
            "None"
        );
        // 90.5 s truncates to 90 before `delta_time_to_string` sees it.
        assert_eq!(
            timed_string(&t, &timed, ServerTime(1000.0), &seconds),
            "90s"
        );
        assert_eq!(
            timed_string(&t, &timed, ServerTime(1091.0), &seconds),
            "Finished"
        );
    }

    /// The contact is the end npc only once the contract is under way.
    #[test]
    fn the_contact_is_the_end_npc_only_once_the_contract_is_under_way() {
        let both = Contract {
            name_npc_start: "Aun Tanua".into(),
            name_npc_end: "Nuhmudira".into(),
            ..Contract::default()
        };
        assert!(!contact_is_the_end_npc(&both, stage::AVAILABLE));
        assert!(contact_is_the_end_npc(&both, stage::IN_PROGRESS));
        assert!(
            !contact_is_the_end_npc(&both, stage::DONE),
            "stage 3 points back at the start"
        );
        assert!(contact_is_the_end_npc(&both, stage::FIRST_SUBSTAGE));
        // No end NPC at all: always the start, whatever the stage.
        let start_only = Contract {
            name_npc_end: String::new(),
            ..both.clone()
        };
        assert!(!contact_is_the_end_npc(&start_only, stage::IN_PROGRESS));
        // No start NPC: the end one stands in even at stage 1.
        let end_only = Contract {
            name_npc_start: String::new(),
            ..both
        };
        assert!(contact_is_the_end_npc(&end_only, stage::AVAILABLE));
    }

    /// Oracle: §3 — the whole table (0x0314) replaces, one tracker (0x0315) merges, and both stamp
    /// the local arrival time.
    #[test]
    fn the_tracker_table_replaces_or_merges_and_stamps_arrival() {
        let mut tbl = ContractTrackerTable::default();
        tbl.replace(
            vec![
                ContractTracker {
                    contract_id: 1,
                    ..ContractTracker::default()
                },
                ContractTracker {
                    contract_id: 2,
                    ..ContractTracker::default()
                },
            ],
            ServerTime(50.0),
        );
        assert_eq!(tbl.trackers.len(), 2);
        assert_eq!(tbl.trackers[&1].time_of_server_update, 50.0);

        tbl.update(
            ContractTracker {
                contract_id: 3,
                contract_stage: 2,
                ..ContractTracker::default()
            },
            ServerTime(60.0),
        );
        assert_eq!(tbl.trackers.len(), 3);
        assert_eq!(tbl.trackers[&3].time_of_server_update, 60.0);

        tbl.replace(vec![], ServerTime(70.0));
        assert!(
            tbl.trackers.is_empty(),
            "the whole table replaces rather than merging"
        );
        assert!(!tbl.remove(3));
    }

    fn wire(id: u32, stage: u32) -> dereth_protocol::social::ContractTracker {
        dereth_protocol::social::ContractTracker {
            version: 0,
            contract_id: id,
            contract_stage: stage,
            time_when_done: 0.0,
            time_when_repeats: 0.0,
        }
    }

    /// One tracker adds updates or removes and a delete for an unknown id inserts.
    #[test]
    fn one_tracker_adds_updates_or_removes_and_a_delete_for_an_unknown_id_inserts() {
        use dereth_protocol::social::SocialSendClientContractTracker;
        let mut w = World::default();
        let msg = |t, del: i32| SocialSendClientContractTracker {
            tracker: t,
            delete_contract: del,
            set_as_display_contract: 0,
        };

        // not held, not a delete: insert, stamped.
        assert_eq!(
            w.recv_contract_tracker(&msg(wire(7, 1), 0), ServerTime(10.0)),
            ContractUpdate::Added
        );
        assert_eq!(w.contract_trackers()[&7].time_of_server_update, 10.0);

        // held: stage and the two doubles are written in place and re-stamped.
        assert_eq!(
            w.recv_contract_tracker(&msg(wire(7, 2), 0), ServerTime(20.0)),
            ContractUpdate::Updated
        );
        assert_eq!(w.contract_trackers()[&7].contract_stage, 2);
        assert_eq!(w.contract_trackers()[&7].time_of_server_update, 20.0);

        // held and a delete: gone.
        assert_eq!(
            w.recv_contract_tracker(&msg(wire(7, 2), 1), ServerTime(30.0)),
            ContractUpdate::Removed
        );
        assert!(w.contract_trackers().is_empty());

        // **The delete-miss branch jumps into the add arm**: a delete for an id the client does not hold
        // *inserts* it. ACE never sends that shape, and the arm is retail's all the same.
        assert_eq!(
            w.recv_contract_tracker(&msg(wire(9, 3), 1), ServerTime(40.0)),
            ContractUpdate::Added
        );
        assert_eq!(w.contract_trackers().len(), 1);
    }

    /// The whole table replaces and every record is stamped.
    #[test]
    fn the_whole_table_replaces_and_every_record_is_stamped() {
        use dereth_protocol::archive::PackedHash;
        use dereth_protocol::social::SocialSendClientContractTrackerTable;
        let mut w = World::default();
        assert_eq!(
            w.recv_contract_tracker(
                &dereth_protocol::social::SocialSendClientContractTracker {
                    tracker: wire(1, 1),
                    delete_contract: 0,
                    set_as_display_contract: 0,
                },
                ServerTime(1.0)
            ),
            ContractUpdate::Added
        );

        let m = SocialSendClientContractTrackerTable(PackedHash {
            table_size: 32,
            entries: vec![(4, wire(4, 2)), (5, wire(5, 3))],
        });
        assert_eq!(w.recv_contract_tracker_table(&m, ServerTime(50.0)), 2);
        assert_eq!(
            w.contract_trackers().keys().copied().collect::<Vec<_>>(),
            vec![4, 5],
            "the table replaces; contract 1 is gone"
        );
        for t in w.contract_trackers().values() {
            assert_eq!(t.time_of_server_update, 50.0);
        }
    }

    /// Abandoning sends the id and removes nothing locally.
    #[test]
    fn abandoning_sends_the_id_and_removes_nothing_locally() {
        use crate::RecordingRequests;
        let mut w = World::default();
        w.contracts.update(
            ContractTracker {
                contract_id: 7,
                ..ContractTracker::default()
            },
            ServerTime(0.0),
        );

        let mut req = RecordingRequests::default();
        assert!(
            w.abandon_contract(&mut req, 0).is_none(),
            "contract_id 0 sends nothing"
        );
        assert!(req.0.is_empty());

        assert!(w.abandon_contract(&mut req, 7).is_some());
        assert_eq!(req.0.len(), 1);
        assert!(
            w.contract_trackers().contains_key(&7),
            "the tracker survives until the shard answers with a 0x0315 delete"
        );
    }
}
