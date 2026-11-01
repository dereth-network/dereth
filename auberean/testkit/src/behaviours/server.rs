//! The server: what it answers a client with, as the January 2017 servers answered.
//!
//! These rows hold the server's side of the recorded sessions: what a server sent, in what
//! order, on which queue, for each thing a client asked. The server's internals (its store, its
//! settings) are tooling.

use super::{Behaviour, Evidence, Tier, RETAIL, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "server.characters.outlive-a-restart",
        says: "An account's characters, with their choices, looks and deletions, are the same at \
               every login, across the server stopping and starting again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSIST"),
        station: "yalain-store::cpu::persistence::characters_and_their_deletions_outlive_reopening_the_database",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world-db.release-notes-from-the-change-log",
        says: "A world database release's notes are its change log's sections from after the \
               last release up to the release, newest first, with the archive's SHA-256; a \
               release the change log has no section for is not made.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WDB-RELEASE-NOTES"),
        station: "yalain-world-db::cpu::release::the_notes_are_the_change_log_s_sections_since_the_last_release_newest_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world-db.release-the-same-bytes-for-the-same-files",
        says: "A world database release built twice from the same files is the same archive, byte \
               for byte: its entries in path order, stored, with one fixed date, so anyone can \
               rebuild a release and check its SHA-256.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WDB-RELEASE-REPRO"),
        station: "yalain-world-db::cpu::release::a_release_built_twice_from_the_same_files_is_the_same_bytes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.state-kept-between-sessions",
        says: "Where a character stands when it leaves the world, its qualities and its options are \
               kept, and it enters next time as it left, across the server stopping and starting \
               again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORLD-STATE"),
        station: "yalain-store::cpu::persistence::a_characters_world_state_outlives_reopening_the_database_and_goes_with_it",
        tier: Tier::Cpu,
    },
];
