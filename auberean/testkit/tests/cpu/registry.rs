//! The behaviour registry is well formed: ids unique, in order and shaped `subject.thing.claim`;
//! evidence handles unique and opaque; public text free of addresses and symbols; stations shaped
//! as test paths; divergences named exactly on this-client rows.
//! Behaviour: none (the registry's own consistency)

use auberean_testkit::behaviours::{all, Evidence, Tier, RETAIL, SUBJECTS, THIS_CLIENT, TOOLING};
use auberean_testkit::divergences::{self, DIVERGENCES};

fn is_id_part(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[test]
fn every_id_is_unique_ordered_and_dotted_lower_case() {
    let mut seen = std::collections::BTreeSet::new();
    for s in SUBJECTS {
        assert!(
            s.rows.windows(2).all(|w| w[0].id < w[1].id),
            "{} rows out of order",
            s.name
        );
        for b in s.rows {
            let parts: Vec<&str> = b.id.split('.').collect();
            assert!(
                parts.len() >= 3 && parts.iter().all(|p| is_id_part(p)),
                "{}",
                b.id
            );
            assert_eq!(parts[0], s.name, "{} filed under {}", b.id, s.name);
            assert!(seen.insert(b.id), "{} repeated", b.id);
        }
    }
    assert!(SUBJECTS.windows(2).all(|w| w[0].name < w[1].name));
}

#[test]
fn every_evidence_handle_is_unique_and_opaque() {
    let mut seen = std::collections::BTreeSet::new();
    for b in all() {
        if let Evidence::Private(h) = b.evidence {
            let rest = h
                .strip_prefix("AC2-EVID-")
                .unwrap_or_else(|| panic!("{}: {h}", b.id));
            assert!(
                !rest.is_empty()
                    && rest
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-'),
                "{}: {h}",
                b.id
            );
            assert!(seen.insert(h), "{h} repeated");
        }
    }
}

#[test]
fn public_text_names_no_address_or_symbol() {
    for b in all() {
        for bad in ["0x", "::", " @ "] {
            assert!(!b.says.contains(bad), "{}: says holds {bad:?}", b.id);
        }
        assert!(b.says.len() >= 40, "{}: says too short", b.id);
    }
}

#[test]
fn every_station_is_a_test_path_in_its_tier() {
    for b in all() {
        let parts: Vec<&str> = b.station.split("::").collect();
        assert!(parts.len() >= 4, "{}: {}", b.id, b.station);
        assert!(
            parts[0] == "yalain"
                || ["auberean-", "yalain-", "athenaeum-"]
                    .iter()
                    .any(|p| parts[0].starts_with(p)),
            "{}: {}",
            b.id,
            b.station
        );
        // A test of an executable's own private parts lives in that executable, on the cpu tier.
        if b.tier == Tier::Cpu && parts[1].starts_with("bin.") {
            continue;
        }
        assert_eq!(parts[1], b.tier.binary(), "{}: {}", b.id, b.station);
    }
}

#[test]
fn a_divergence_is_named_exactly_on_this_client_rows() {
    for b in all() {
        assert!(
            [RETAIL, THIS_CLIENT, TOOLING].contains(&b.since),
            "{}: {}",
            b.id,
            b.since
        );
        match b.divergence {
            Some(d) => {
                assert_eq!(b.since, THIS_CLIENT, "{}", b.id);
                assert!(
                    divergences::lookup(d).is_some(),
                    "{}: {d} is not in the register",
                    b.id
                );
            }
            None => assert_ne!(b.since, THIS_CLIENT, "{} names no divergence", b.id),
        }
    }
    for d in DIVERGENCES {
        let name =
            d.id.strip_prefix("CD2-")
                .unwrap_or_else(|| panic!("{}", d.id));
        assert!(is_id_part(name), "{}", d.id);
    }
    assert!(DIVERGENCES.windows(2).all(|w| w[0].id < w[1].id));
}
