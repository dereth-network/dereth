//! The behaviour registry: what this client claims to do, and which claims a test asserts.
//!
//! # Why a registry rather than a file header
//!
//! A test whose claim lives only in a `//!` block naming its oracle has two problems:
//!
//! * **The claims cannot be counted.** "How many behaviours does this client claim, and how many
//!   does a test assert?" is a question no tool can answer over prose, and every attempt ends up
//!   as a separate instrument counting one corner.
//! * **The oracle is private evidence**, so the claim cannot be published with the code.
//!
//! A [`Behaviour`] is that header, split into the part anyone may read -- an id, a sentence, and
//! when it was true -- and an [`Evidence`] handle that resolves in the private repository. A
//! scenario declares the ids it asserts and then asserts them; [`census`] counts the registry's
//! ids against those declarations and names the gap, in both directions.
//!
//! **An unasserted behaviour is a hole with a name.** That is the whole argument
//! `dereth_client_net::client_session::testing`'s `ReplayReport::unmodelled` makes about the third state -- not a pass,
//! not a failure, and never silent -- applied to claims instead of to blobs.
//!
//! # Rows
//!
//! `evidence` is an id in the private evidence repository and **never an address**: `AC-EVID-`
//! followed by the key the archive files the proof under. [`the ids are unique`](census) is
//! asserted by the registry's own test.
//!
//! A row is a claim and not a file: a test that makes two or three independent claims -- a melee
//! swing's readiness, the swing itself and the mode toggle, say -- has that many rows. What does
//! **not** become a row is anything about the retail program's internals rather than its
//! behaviour. That is evidence, and it lives behind the `AC-EVID-` handle.
//!
//! `since` is the build the claim was read off: retail, or [`THIS_CLIENT`] where this client
//! deliberately does something retail did not. A row of the second kind names the published
//! divergence it is part of (`divergence: "CD-001"`, from [`crate::divergences`]); the
//! divergence module's test requires that, and forbids it on a retail row. A third stamp,
//! [`TOOLING`], marks a developer affordance: recorded and asserted, but not published, so it
//! names no divergence either.
//!
//! # Where a row is asserted
//!
//! `station` names the one live test that asserts the row, as a durable test path
//! `<package>::<binary>::<module path>::<fn>`, so it survives a crate moving on disk and a reader
//! can type it as a test filter. Most rows are asserted by a scenario in this crate
//! (`dereth-testkit::cpu::chat::scenario_...`); the rest by a crate's own test, which names the row
//! on a `/// Behaviour: <id>` line (`dereth-client::gpu::...`). `cargo xtask test-lint` checks
//! both directions: every station is a live test, and every `Behaviour:` line names a row. Where
//! a claim was first measured is history, and it is kept with the evidence behind the handle.
//!
//! A few stations compare the client against a recording this repository does not carry, a
//! retail window capture, so their tests live outside it. Such a row says so with
//! `private_oracle: true` after its `station`: the claim is published, and so is the fact that its
//! proof is not. `cargo xtask test-lint` holds the mark to the facts: a marked station must be
//! absent from the workspace, and an unmarked one live.
//!
//! # One file per subject
//!
//! The table is one file per subject -- `chat.rs`, `inventory.rs`, `world.rs` and the rest -- each
//! exporting a `ROWS` slice, with [`SUBJECTS`] the list of them, so that work on one subject's
//! scenarios edits that subject's file and its own scenario file and nothing else.
//!
//! The well-formedness a single sorted table would have is asserted across the files
//! instead: each subject's slice is in id order, the subjects are in name order, and no id and no
//! evidence handle is repeated anywhere. [`lookup`] walks the subjects, so it is still one pass
//! over the rows.
//!
//! # The census runs nothing
//!
//! The count is static. A census that ran every scenario of the tier a second time inside itself
//! -- libtest has no after-all hook, so that would be the only way to read a process-global set
//! of assertions and know what had run -- would double the cost of the slow `dat` tier.
//!
//! A scenario's `ALL` entry is a [`Scenario`]: a name, **the behaviour ids it asserts**, and
//! the function. [`run_scenario`] runs one under [`record`] and fails unless declared ==
//! asserted, so a drifting declaration is that scenario's own red; [`census`] compares two sets
//! of static text and runs nothing. A recording is per thread and lives exactly as long as one
//! scenario.

use std::cell::RefCell;
use std::collections::BTreeSet;

/// Where a behaviour's proof lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Evidence {
    /// An id in the private evidence repository. It resolves there and nowhere else; nothing in
    /// this repository may turn it back into an address, which is the point of the indirection.
    Private(&'static str),
    /// A published reference anyone can follow, such as a reference-server source file or a
    /// public protocol document.
    Public(&'static str),
}

impl Evidence {
    /// The handle itself, whichever kind it is.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Private(s) | Self::Public(s) => s,
        }
    }
}

/// Which tier the test asserting this behaviour runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// No retail data file is opened. The recorded captures are committed to the repository and
    /// are not a dat, so a `Cpu` scenario may still replay one.
    Cpu,
    /// The retail dats under `$DERETH_TEST_DAT_DIR` are opened.
    Dat,
    /// A graphics device is opened, and the claim is about what it draws. Only a crate's own test
    /// asserts one; no scenario runs in this tier.
    Gpu,
}

impl Tier {
    /// The test binary this tier's tests live in.
    #[must_use]
    pub const fn binary(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Dat => "dat",
            Self::Gpu => "gpu",
        }
    }
}

/// The package whose scenarios assert most rows; a station in it is a scenario's test.
pub const SCENARIO_PACKAGE: &str = "dereth-testkit";

/// One documented behaviour of the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Behaviour {
    /// The dotted id a test asserts. Stable: renaming one is renaming a claim.
    pub id: &'static str,
    /// What the client does, in words a player would recognise. No address, no internal symbol.
    pub says: &'static str,
    /// Which build the claim was read off.
    pub since: &'static str,
    /// The client divergence this row is part of, as its `CD-###` id in
    /// [`crate::divergences::DIVERGENCES`]: `Some` exactly when `since` is [`THIS_CLIENT`], because
    /// a claim read off this client's own rule is a place where it deliberately differs from
    /// retail, and every such place is published.
    pub divergence: Option<&'static str>,
    /// Where the proof is.
    pub evidence: Evidence,
    /// The live test that asserts this claim, as a durable test path
    /// `<package>::<binary>::<module path>::<fn>`.
    pub station: &'static str,
    /// Whether [`Self::station`] compares against a recording that only the private research
    /// tree holds, so that the published repository does not carry the test.
    pub private_oracle: bool,
    /// Which tier that test runs in.
    pub tier: Tier,
}

impl Behaviour {
    /// Whether a scenario of this crate asserts the row, rather than a crate's own test.
    #[must_use]
    pub fn is_scenario(&self) -> bool {
        self.station
            .strip_prefix(SCENARIO_PACKAGE)
            .and_then(|r| r.strip_prefix("::"))
            .and_then(|r| r.strip_prefix(self.tier.binary()))
            .is_some_and(|r| r.starts_with("::"))
    }
}

/// Build one [`Behaviour`] row.
///
/// The macro exists so that a row reads as the header it replaces and so that a missing field is a
/// compile error rather than a silently defaulted one. There are two optional fields:
/// `divergence`, written straight after `since` on a row whose `since` is [`THIS_CLIENT`] (the
/// registry's own test makes it required there and forbidden everywhere else), and
/// `private_oracle: true`, written straight after `station` on a row whose station is a
/// private-oracle test (see "Where a row is asserted").
#[macro_export]
macro_rules! behaviour {
    (
        id: $id:literal,
        says: $says:literal,
        since: $since:expr,
        $(divergence: $divergence:literal,)?
        evidence: $evidence:expr,
        station: $station:literal,
        $(private_oracle: $private_oracle:literal,)?
        tier: $tier:expr $(,)?
    ) => {
        $crate::behaviours::Behaviour {
            id: $id,
            says: $says,
            since: $since,
            divergence: $crate::behaviour!(@divergence $($divergence)?),
            evidence: $evidence,
            station: $station,
            private_oracle: $crate::behaviour!(@private_oracle $($private_oracle)?),
            tier: $tier,
        }
    };
    (@divergence) => {
        None
    };
    (@divergence $divergence:literal) => {
        Some($divergence)
    };
    (@private_oracle) => {
        false
    };
    (@private_oracle $private_oracle:literal) => {
        $private_oracle
    };
}

/// Retail as it shipped in the final client, acclient 00.00.11.6096 of 12 June 2015, which is the
/// build this project rebuilds. A claim first read off the September 2013 build (00.00.11.4186)
/// carries this stamp too where the two builds behave identically, which is everywhere except the
/// few places the final build changed; those rows were read again against it.
pub(crate) const RETAIL: &str = "retail 2015-06";

/// This client, where it deliberately does something retail did not: the claim is read off this
/// client's own rule rather than off retail.
pub(crate) const THIS_CLIENT: &str = "this client";

/// A developer affordance of this client that a player never meets in a normal session, such as
/// running with no server at all. It is recorded so that a test asserts it, and it is not a
/// published divergence: a row stamped this way names none.
pub(crate) const TOOLING: &str = "tooling";

/// One subject's rows: the file they live in, and the slice it exports.
#[derive(Debug, Clone, Copy)]
pub struct Subject {
    /// The file's stem, which is also the stem of the scenario file the rows are asserted from.
    pub name: &'static str,
    /// That file's rows, in id order.
    pub rows: &'static [Behaviour],
}

pub mod audio;
pub mod camera;
pub mod chat;
pub mod combat;
pub mod frame;
pub mod inventory;
pub mod login;
pub mod magic;
pub mod movement;
pub mod net;
pub mod objects;
pub mod panels;
pub mod presentation;
pub mod rendering;
pub mod selection;
pub mod shell;
pub mod social;
pub mod ui;
pub mod world;

/// Every subject file, in name order.
///
/// **This is the list a new subject is added to**, and with the `mod` line above it is the only
/// part of this file a new subject has to touch. Everything below iterates it; nothing anywhere
/// holds a total.
pub static SUBJECTS: &[Subject] = &[
    Subject {
        name: "audio",
        rows: audio::ROWS,
    },
    Subject {
        name: "camera",
        rows: camera::ROWS,
    },
    Subject {
        name: "chat",
        rows: chat::ROWS,
    },
    Subject {
        name: "combat",
        rows: combat::ROWS,
    },
    Subject {
        name: "frame",
        rows: frame::ROWS,
    },
    Subject {
        name: "inventory",
        rows: inventory::ROWS,
    },
    Subject {
        name: "login",
        rows: login::ROWS,
    },
    Subject {
        name: "magic",
        rows: magic::ROWS,
    },
    Subject {
        name: "movement",
        rows: movement::ROWS,
    },
    Subject {
        name: "net",
        rows: net::ROWS,
    },
    Subject {
        name: "objects",
        rows: objects::ROWS,
    },
    Subject {
        name: "panels",
        rows: panels::ROWS,
    },
    Subject {
        name: "presentation",
        rows: presentation::ROWS,
    },
    Subject {
        name: "rendering",
        rows: rendering::ROWS,
    },
    Subject {
        name: "selection",
        rows: selection::ROWS,
    },
    Subject {
        name: "shell",
        rows: shell::ROWS,
    },
    Subject {
        name: "social",
        rows: social::ROWS,
    },
    Subject {
        name: "ui",
        rows: ui::ROWS,
    },
    Subject {
        name: "world",
        rows: world::ROWS,
    },
];

/// Every documented behaviour, subject by subject.
///
/// The order is the subject order and then each file's own id order. It is stable: a row moves
/// only when its id or its subject changes, and either of those is a change to a claim.
pub fn all() -> impl Iterator<Item = &'static Behaviour> {
    SUBJECTS.iter().flat_map(|s| s.rows.iter())
}

/// How many behaviours the registry documents.
#[must_use]
pub fn count() -> usize {
    SUBJECTS.iter().map(|s| s.rows.len()).sum()
}

/// Look one up.
#[must_use]
pub fn lookup(id: &str) -> Option<&'static Behaviour> {
    all().find(|b| b.id == id)
}

/// Every id, in the table's own order.
#[must_use]
pub fn ids() -> Vec<&'static str> {
    all().map(|b| b.id).collect()
}

/// One scenario, as its file's `ALL` lists it: the name the table gives it, **the behaviour ids it
/// asserts**, and the function.
///
/// The ids are static data, so knowing what a scenario asserts does not mean running it: the
/// scenario declares its ids, [`run_scenario`] checks the declaration against the run for
/// that one scenario, and the census compares two sets without running anything.
pub type Scenario = (&'static str, &'static [&'static str], fn());

thread_local! {
    /// The ids noted since [`record`] began on this thread, or `None` when nothing is recording.
    ///
    /// **Per thread, and only while a scenario runs.** A process-global set cannot say *which* scenario asserted an id, which is
    /// what makes a declaration checkable; and the `cpu` binary runs its tests in parallel, so a
    /// shared set would have mixed two scenarios' assertions together.
    static RECORDING: RefCell<Option<BTreeSet<&'static str>>> = const { RefCell::new(None) };
}

/// Run `f` and answer with the behaviour ids it asserted.
///
/// # Panics
/// Panics when a recording is already open on this thread: recordings do not nest, and a nested
/// one would hand its ids to the wrong scenario.
pub fn record(f: impl FnOnce()) -> BTreeSet<&'static str> {
    /// Ends the recording however `f` ends -- a scenario that fails its own assertion unwinds, and
    /// with `--test-threads=1` the next scenario runs on this same thread.
    struct Open;
    impl Drop for Open {
        fn drop(&mut self) {
            RECORDING.with(|r| *r.borrow_mut() = None);
        }
    }

    RECORDING.with(|r| {
        let mut slot = r.borrow_mut();
        assert!(
            slot.is_none(),
            "a scenario is already recording on this thread"
        );
        *slot = Some(BTreeSet::new());
    });
    let open = Open;
    f();
    let ids = RECORDING
        .with(|r| r.borrow_mut().take())
        .unwrap_or_default();
    drop(open);
    ids
}

/// Run one scenario of `all` by name, and assert that the ids it asserted are **exactly** the ones
/// its entry declares.
///
/// This is what every `#[test] fn scenario_*` in a scenario file calls. It is the check that makes
/// a static census honest: a scenario that asserts a claim it did not declare, or declares one it
/// does not assert, fails as itself and names both gaps, rather than surfacing as an arithmetic
/// mismatch in a census that ran everything.
///
/// # Panics
/// Panics when `name` is in no entry of `all`, and when the two sets differ.
pub fn run_scenario(all: &[Scenario], name: &str) {
    let (_, declared, f) = all.iter().find(|(n, _, _)| *n == name).unwrap_or_else(|| {
        panic!(
            "no scenario named {name:?} is listed in this file's ALL, so the census cannot see it"
        )
    });
    let asserted = record(*f);
    let declared: BTreeSet<&'static str> = declared.iter().copied().collect();
    let undeclared: Vec<&str> = asserted.difference(&declared).copied().collect();
    let unasserted: Vec<&str> = declared.difference(&asserted).copied().collect();
    assert!(
        undeclared.is_empty() && unasserted.is_empty(),
        "the scenario {name} asserted {asserted:?} and its ALL entry declares {declared:?}: it \
         asserted {undeclared:?} without declaring them, and declares {unasserted:?} without \
         asserting them"
    );
}

/// Record `id` as asserted. Called by `HeadlessClient::assert_behaviour`.
///
/// Outside a [`record`] this validates the id and counts nothing, which is what a scenario driven
/// by something other than [`run_scenario`] is: not coverage.
///
/// # Panics
/// Panics when `id` is in no subject file. An assertion against an undocumented id is a typo or
/// a claim nobody wrote down, and either way it must not read as coverage.
pub fn note_asserted(id: &'static str) {
    assert!(
        lookup(id).is_some(),
        "no behaviour is documented with the id {id:?}; add it to dereth_testkit::behaviours \
         before asserting it"
    );
    RECORDING.with(|r| {
        if let Some(open) = r.borrow_mut().as_mut() {
            open.insert(id);
        }
    });
}

/// Documented versus declared, with both gaps named. Only the rows a scenario asserts are counted:
/// a row asserted by a crate's own test is checked by `cargo xtask test-lint` instead.
///
/// **Both directions.** [`note_asserted`] panics on an id no row documents, but a declaration is
/// static text, so a typo in one is a claim that exists in a table and nowhere in the registry,
/// and it has to be reported rather than quietly dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Census {
    /// How many behaviours the registry documents for this tier's scenarios.
    pub documented: usize,
    /// How many ids the tier's scenarios declare.
    pub declared: usize,
    /// Documented for this tier and declared by no scenario, in registry order. **This is the
    /// output that matters**: an unasserted behaviour is a hole with a name.
    pub undeclared: Vec<&'static str>,
    /// Declared by a scenario and documented by no row of this tier, in declaration order.
    pub undocumented: Vec<&'static str>,
}

impl Census {
    /// Whether the documented set and the declared set are the same set.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.undeclared.is_empty() && self.undocumented.is_empty()
    }
}

impl std::fmt::Display for Census {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "behaviours: {} documented, {} declared",
            self.documented, self.declared
        )?;
        if self.complete() {
            return write!(f, "  documented == declared");
        }
        for (what, ids) in [
            ("undeclared", &self.undeclared),
            ("undocumented", &self.undocumented),
        ] {
            if ids.is_empty() {
                continue;
            }
            writeln!(f, "  {what}:")?;
            for id in ids {
                writeln!(f, "    {id}")?;
            }
        }
        Ok(())
    }
}

/// Count the scenario rows of `tier` against the ids the tier's scenarios **declare**.
///
/// Pass `None` for `tier` to count every scenario row. Nothing runs: `declared` is the union of
/// the tier's `ALL` entries, which is static data, and this is set arithmetic over it.
#[must_use]
pub fn census(tier: Option<Tier>, declared: &BTreeSet<&'static str>) -> Census {
    let rows: Vec<&Behaviour> = all()
        .filter(|b| b.is_scenario() && tier.is_none_or(|t| b.tier == t))
        .collect();
    let documented: BTreeSet<&'static str> = rows.iter().map(|b| b.id).collect();
    Census {
        documented: rows.len(),
        declared: declared.len(),
        undeclared: rows
            .iter()
            .filter(|b| !declared.contains(b.id))
            .map(|b| b.id)
            .collect(),
        undocumented: declared.difference(&documented).copied().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_is_well_formed() {
        let mut ids = BTreeSet::new();
        let mut evidence = BTreeSet::new();
        for b in all() {
            assert!(ids.insert(b.id), "duplicate behaviour id {:?}", b.id);
            assert!(
                evidence.insert(b.evidence.id()),
                "two behaviours share the evidence id {:?}",
                b.evidence.id()
            );
            assert!(
                b.id.chars()
                    .all(|c| c.is_ascii_lowercase() || "-.".contains(c)),
                "{:?} is not a dotted lower-case id",
                b.id
            );
            assert!(
                b.says.len() > 40,
                "{:?}'s sentence is too short to be a claim",
                b.id
            );
            assert!(
                b.station.split("::").count() >= 3
                    && b.station
                        .split("::")
                        .all(|p| !p.is_empty() && !p.contains('/')),
                "{:?}'s station {:?} is not a durable test path",
                b.id,
                b.station
            );
            assert!(
                b.is_scenario() || !b.station.starts_with(SCENARIO_PACKAGE),
                "{:?} is asserted by a scenario of another tier than its own",
                b.id
            );
            assert!(
                b.tier != Tier::Gpu || !b.is_scenario(),
                "{:?}: no scenario runs in the gpu tier",
                b.id
            );
            assert!(
                !b.private_oracle
                    || (!b.is_scenario() && matches!(b.evidence, Evidence::Private(_))),
                "{:?}: a private-oracle station is a crate's own test, and its evidence is private",
                b.id
            );
        }
        assert_eq!(ids.len(), count(), "an id was counted twice");
    }

    /// **The order a single table would have had for free.** The registry is split by subject, so
    /// "the whole registry is sorted" is not the property worth having: a subject file is what a
    /// change edits, and what must not drift is the order *inside* it. Both halves are asserted -- each file in id order, the files themselves in name order -- so a row appended at
    /// the bottom of a subject, or a subject spliced into the middle of [`SUBJECTS`], is still a red.
    #[test]
    fn every_subject_file_is_in_id_order_and_the_subjects_are_in_name_order() {
        for s in SUBJECTS {
            let ids: Vec<&str> = s.rows.iter().map(|b| b.id).collect();
            let mut sorted = ids.clone();
            sorted.sort_unstable();
            assert_eq!(ids, sorted, "behaviours/{}.rs is not in id order", s.name);
            assert!(!s.rows.is_empty(), "behaviours/{}.rs has no rows", s.name);
        }
        let names: Vec<&str> = SUBJECTS.iter().map(|s| s.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "SUBJECTS is not in name order");
        let unique: BTreeSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), names.len(), "a subject is listed twice");
    }

    /// **A subject file that is written and never listed documents nothing.** The registry's own
    /// count is the sum of the slices [`SUBJECTS`] names, so a `pub mod` with no `Subject` row
    /// would be invisible to the census rather than a shortfall in it. This is the one thing the
    /// split could lose silently, so it is the one thing asserted directly against the source.
    #[test]
    fn every_subject_module_is_listed_in_subjects() {
        let listed: BTreeSet<&str> = SUBJECTS.iter().map(|s| s.name).collect();
        let source = include_str!("mod.rs");
        let declared: BTreeSet<&str> = source
            .lines()
            .filter_map(|l| l.strip_prefix("pub mod ").and_then(|r| r.strip_suffix(';')))
            .collect();
        assert!(
            !declared.is_empty(),
            "no subject module is declared; the scan is broken"
        );
        assert_eq!(
            declared, listed,
            "a subject module is declared but not listed in SUBJECTS"
        );
    }

    /// **The first twenty behaviours the registry documented** are still documented, and each is
    /// still asserted by a scenario. They are asserted by name rather than by counting the table,
    /// so a row that is deleted or renamed is a red while adding a row changes nothing.
    #[test]
    fn the_first_twenty_behaviours_are_still_asserted_by_scenarios() {
        // A pinned total would be the one line every change adding a row has to edit, and it
        // would prove nothing the census does not prove better (documented == declared, per tier).
        let seed = [
            "allegiance.member-login.becomes-a-chat-line",
            "chargen.every-wizard-page-lays-out",
            "chat.emote.is-drawn-as-name-then-text",
            "chat.speech.earshot-and-squelch-both-gate",
            "chat.talk-focus.one-authoritative-value",
            "container.ground.contents-become-a-pickup",
            "frame.fourteen-steps-in-order-every-frame",
            "headless.a-whole-client-with-no-device-or-window",
            "login.replay.character-set-is-the-recordings-own",
            "movement.run.reports-walk-with-the-run-hold-key",
            "notice.bubble.clears-itself-after-five-seconds",
            "notice.refusal.is-a-bubble-and-not-a-chat-line",
            "object.set-state.visible-list-follows",
            "objects.force-objdesc.leaves-on-the-control-queue",
            "physics.enter-world.refuses-a-body-inside-a-building",
            "player.identity.comes-from-the-shard",
            "spellbar.favorite.model-and-wire-move-together",
            "spellbook.click.reveals-and-selects-the-row",
            "trade.drop.sends-add-to-trade-and-marks-the-item",
            "use.progress-notice.names-the-object",
        ];
        for id in seed {
            let b = lookup(id).unwrap_or_else(|| panic!("the first-documented row {id} has gone"));
            assert!(
                b.is_scenario(),
                "{id} is no longer asserted by a scenario: {}",
                b.station
            );
        }
    }

    /// **No retail citation reaches a published field.** A behaviour's prose is what a player
    /// would recognise; the address is in the private repository behind the evidence id.
    #[test]
    fn no_documented_sentence_carries_a_retail_citation() {
        for b in all() {
            for (what, text) in [("says", b.says), ("id", b.id), ("since", b.since)] {
                assert!(
                    !text.contains("0x"),
                    "{}'s {what} carries a hex literal: {text:?}",
                    b.id
                );
                assert!(
                    !text.contains(" @ "),
                    "{}'s {what} carries an address citation: {text:?}",
                    b.id
                );
                assert!(
                    !text.contains("::"),
                    "{}'s {what} names an internal symbol: {text:?}",
                    b.id
                );
            }
            assert!(
                matches!(b.evidence, Evidence::Private(_) | Evidence::Public(_)),
                "{} has no evidence handle",
                b.id
            );
            assert!(
                b.evidence.id().starts_with("AC-EVID-"),
                "{}'s evidence id {:?} is not a private-repository handle",
                b.id,
                b.evidence.id()
            );
        }
    }

    #[test]
    fn an_undocumented_id_cannot_be_asserted() {
        let e = std::panic::catch_unwind(|| note_asserted("no.such.behaviour"));
        assert!(
            e.is_err(),
            "asserting an undocumented id must not read as coverage"
        );
    }

    #[test]
    fn a_census_of_nothing_names_every_id_it_counted() {
        // A census against an empty declaration must report the whole tier, not zero of zero --
        // the failure mode the `*_census.rs` instruments had.
        let c = census(Some(Tier::Dat), &BTreeSet::new());
        assert!(
            c.documented > 0,
            "the Dat tier documents nothing; the filter is dead"
        );
        assert_eq!(c.declared, 0);
        assert_eq!(c.undeclared.len(), c.documented);
        assert!(c.undocumented.is_empty());
        assert!(!c.complete());
        assert!(format!("{c}").contains("documented"));
    }

    /// **A declaration that names nothing in the registry is reported, not dropped.** It is the
    /// half the running census could not have: an id that no row documents used to panic inside
    /// `note_asserted` before it could be counted.
    #[test]
    fn a_declared_id_that_no_row_documents_is_named() {
        let declared = BTreeSet::from(["no.such.behaviour"]);
        let c = census(Some(Tier::Dat), &declared);
        assert_eq!(c.undocumented, ["no.such.behaviour"]);
        assert!(!c.complete());
        assert!(format!("{c}").contains("undocumented"));
    }

    /// Every scenario row, declared: the shape both tiers' census tests are in when they are green.
    #[test]
    fn a_census_of_everything_is_complete_and_says_so() {
        let declared: BTreeSet<&'static str> =
            all().filter(|b| b.is_scenario()).map(|b| b.id).collect();
        let c = census(None, &declared);
        assert!(c.complete(), "{c}");
        assert_eq!(c.documented, declared.len());
        assert_eq!(c.declared, declared.len());
        assert!(format!("{c}").contains("documented == declared"));
    }

    /// A row a crate's own test asserts is not a scenario's to declare: the census leaves it out,
    /// so a crate test's row never reads as a hole in a scenario tier.
    #[test]
    fn a_row_a_crate_test_asserts_is_outside_the_census() {
        let crate_rows: Vec<&Behaviour> = all().filter(|b| !b.is_scenario()).collect();
        assert!(
            !crate_rows.is_empty(),
            "no row is asserted by a crate's own test; the split is dead"
        );
        let c = census(None, &BTreeSet::new());
        for b in crate_rows {
            assert!(!c.undeclared.contains(&b.id), "{} was counted", b.id);
        }
        assert_eq!(
            c.documented + all().filter(|b| !b.is_scenario()).count(),
            count()
        );
    }

    #[test]
    fn a_recording_holds_what_the_scenario_noted_and_ends_with_it() {
        let id = all().next().expect("the registry has rows").id;
        assert_eq!(record(|| note_asserted(id)), BTreeSet::from([id]));
        // Outside a recording a note is validated and counted by nobody, so the next scenario
        // starts from nothing.
        note_asserted(id);
        assert!(record(|| {}).is_empty());
    }

    /// A scenario that fails unwinds, and with `--test-threads=1` the next one runs on the same
    /// thread. A recording that outlived the failure would hand its ids to that next scenario.
    #[test]
    fn a_scenario_that_fails_still_ends_its_recording() {
        let id = all().next().expect("the registry has rows").id;
        let e = std::panic::catch_unwind(|| {
            record(|| {
                note_asserted(id);
                panic!("the scenario failed");
            })
        });
        assert!(e.is_err());
        assert!(
            record(|| {}).is_empty(),
            "a failed recording leaked into the next scenario"
        );
    }

    #[test]
    fn a_scenario_that_asserts_what_it_did_not_declare_is_a_red() {
        fn asserts_the_first_row() {
            note_asserted(all().next().expect("the registry has rows").id);
        }
        let table: Vec<Scenario> = vec![("asserts_the_first_row", &[], asserts_the_first_row)];
        let e = std::panic::catch_unwind(|| run_scenario(&table, "asserts_the_first_row"));
        assert!(
            e.is_err(),
            "an undeclared assertion must not read as coverage"
        );
    }

    #[test]
    fn a_scenario_that_declares_what_it_did_not_assert_is_a_red() {
        fn asserts_nothing() {}
        let id = all().next().expect("the registry has rows").id;
        let declared: &'static [&'static str] = Box::leak(vec![id].into_boxed_slice());
        let table: Vec<Scenario> = vec![("asserts_nothing", declared, asserts_nothing)];
        let e = std::panic::catch_unwind(|| run_scenario(&table, "asserts_nothing"));
        assert!(
            e.is_err(),
            "a claim nothing asserts must not read as coverage"
        );
    }
}
