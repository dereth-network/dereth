//! The server's divergence register is well formed: ids named in order, every row with an opaque
//! evidence handle and its statements, every behavior row it names in the registry, and the
//! published list what the register generates.
//! Behaviour: none (the register's own consistency)

use std::collections::BTreeSet;
use std::path::Path;

use auberean_testkit::server_divergences::{
    self as divergences, station, Kind, DIVERGENCES, PUBLISHED, REWRITE_VAR,
};

#[test]
fn every_divergence_is_named_once_and_in_order() {
    let mut seen = BTreeSet::new();
    for d in DIVERGENCES {
        assert!(
            divergences::is_id(d.id),
            "{} is not `YD-` and lowercase words joined by hyphens",
            d.id
        );
        assert!(seen.insert(d.id), "{} is listed twice", d.id);
        assert_eq!(divergences::lookup(d.id), Some(d));
    }
    for pair in DIVERGENCES.windows(2) {
        assert!(
            pair[0].id < pair[1].id,
            "the divergences stand in alphabetical order: {} before {}",
            pair[0].id,
            pair[1].id
        );
    }
}

/// A named id is `YD-` and lowercase words of letters and digits joined by single hyphens,
/// beginning with a letter; a number is not a name.
#[test]
fn a_divergences_id_is_lowercase_words_joined_by_hyphens() {
    for id in ["YD-talk-has-a-limit", "YD-x2-echo", "YD-a"] {
        assert!(divergences::is_id(id), "{id}");
    }
    for id in [
        "YD-001",
        "YD-Talk-limit",
        "YD-talk--limit",
        "YD-talk-",
        "YD-",
        "talk-limit",
        "CD2-talk-limit",
        "YD-talk_limit",
        "YD-2x-echo",
    ] {
        assert!(!divergences::is_id(id), "{id}");
    }
}

#[test]
fn every_divergence_carries_an_opaque_evidence_handle() {
    for d in DIVERGENCES {
        assert!(
            divergences::is_handle(d.evidence),
            "{}: {:?} is not `AC2-EVID-` and capital letters, digits and hyphens",
            d.id,
            d.evidence
        );
    }
    assert!(divergences::is_handle("AC2-EVID-SRV-TALK-LIMIT"));
    for bad in [
        "AC2-EVID-",
        "AC-EVID-SRV",
        "AC2-EVID-srv-talk",
        "AC2-EVID-SRV TALK",
    ] {
        assert!(!divergences::is_handle(bad), "{bad}");
    }
}

/// The register is public text: each statement says something, in words a player or an operator
/// would recognize, with no address, no internal name and nothing that would break the table.
#[test]
fn every_divergence_states_what_retail_did_what_yalain_does_and_why_in_plain_words() {
    for d in DIVERGENCES {
        for (what, text) in [
            ("title", d.title),
            ("retail", d.retail),
            ("yalain", d.yalain),
            ("why", d.why),
        ] {
            assert!(
                text.len() > 15,
                "{}'s {what} is too short to say anything",
                d.id
            );
            for bad in ["0x", " @ ", "::", "`", "|", "\n", "  "] {
                assert!(
                    !text.contains(bad),
                    "{}'s {what} carries {bad:?}: {text:?}",
                    d.id
                );
            }
        }
        assert!(
            !d.title.ends_with('.'),
            "{}'s title is a name, not a sentence",
            d.id
        );
        for (what, text) in [("retail", d.retail), ("yalain", d.yalain), ("why", d.why)] {
            assert!(text.ends_with('.'), "{}'s {what} is a sentence", d.id);
        }
    }
}

/// Every behavior row a divergence names is a row of the registry, so its test is a live one; a
/// divergence that the recordings show too seldom to model has no behavior of Yalain's to assert,
/// and names none.
#[test]
fn every_behavior_row_a_divergence_names_is_in_the_registry() {
    for d in DIVERGENCES {
        for row in d.rows {
            assert!(
                station(row).is_some(),
                "{} names the behavior row {row}, which the registry does not hold",
                d.id
            );
        }
        if d.kind == Kind::Unmodeled {
            assert!(d.rows.is_empty(), "{} models nothing to assert", d.id);
        }
    }
}

/// **The published list is what the register generates.** A changed row, a new divergence or a
/// moved test changes the file, and the change is a diff to read; with the rewrite variable set
/// the file is rewritten instead.
#[test]
fn the_published_list_is_what_the_register_generates() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(PUBLISHED);
    let want = divergences::render();
    if divergences::rewriting() {
        std::fs::write(&path, want.as_bytes())
            .unwrap_or_else(|e| panic!("rewriting {}: {e}", path.display()));
        return;
    }
    let have = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} did not load: {e}", path.display()))
        .replace("\r\n", "\n");
    assert!(
        have == want,
        "{} is not what the register generates; rewrite it with {REWRITE_VAR}=1 and read the diff",
        path.display()
    );
}
