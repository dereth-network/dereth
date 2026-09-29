//! Contracts for attack notification width.
//! Fixture: shared recorded messages and synthetic state.

use crate::common::attack_notifications::*;

/// Behaviour: combat.notification.an-attack-notice-decodes-with-either-attack-conditions-width
#[test]
fn every_recorded_body_is_the_clients_read_plus_one_trailing_zero_dword() {
    let attackers = bodies_of(ATTACKER);
    let defenders = bodies_of(DEFENDER);
    assert!(!attackers.is_empty());
    assert!(!defenders.is_empty());

    let mut checked = 0usize;
    for b in &attackers {
        let end = client_end(&b.bytes, 24);
        assert_eq!(
            b.bytes.len() - end,
            4,
            "{} idx {}: the client's read of `0x01B1` must leave exactly the one dword ACE adds",
            b.scenario,
            b.idx
        );
        assert_eq!(
            b.bytes[end..],
            [0u8; 4],
            "{} idx {}: the trailing dword is zero",
            b.scenario,
            b.idx
        );
        let (m, remaining, _) = client_read::<AttackerNotification>(&b.bytes);
        assert_eq!(
            remaining, 0,
            "{} idx {}: the codec reads the high dword",
            b.scenario, b.idx
        );
        assert_eq!(m.attack_conditions_high, 0);
        let (short, _, _) = client_read::<AttackerNotification>(&b.bytes[..end]);
        assert_eq!(
            short, m,
            "{} idx {}: the four-byte form reads the same",
            b.scenario, b.idx
        );
        let round = dereth_protocol::write_body::<AttackerNotification>(&m)
            .expect("a decoded message must re-encode");
        assert_eq!(
            round, b.bytes,
            "{} idx {}: re-encoding must be the recorded body, high dword included",
            b.scenario, b.idx
        );
        checked += 1;
    }
    for b in &defenders {
        let end = client_end(&b.bytes, 28);
        assert_eq!(
            b.bytes.len() - end,
            4,
            "{} idx {}: the client's read of `0x01B2` must leave exactly the one dword ACE adds",
            b.scenario,
            b.idx
        );
        assert_eq!(
            b.bytes[end..],
            [0u8; 4],
            "{} idx {}: the trailing dword is zero",
            b.scenario,
            b.idx
        );
        let (m, remaining, _) = client_read::<DefenderNotification>(&b.bytes);
        assert_eq!(
            remaining, 0,
            "{} idx {}: the codec reads the high dword",
            b.scenario, b.idx
        );
        assert_eq!(m.attack_conditions_high, 0);
        let (short, _, _) = client_read::<DefenderNotification>(&b.bytes[..end]);
        assert_eq!(
            short, m,
            "{} idx {}: the four-byte form reads the same",
            b.scenario, b.idx
        );
        let round = dereth_protocol::write_body::<DefenderNotification>(&m)
            .expect("a decoded message must re-encode");
        assert_eq!(
            round, b.bytes,
            "{} idx {}: re-encoding must be the recorded body, high dword included",
            b.scenario, b.idx
        );
        checked += 1;
    }
    assert_eq!(checked, attackers.len() + defenders.len());
}

#[test]
fn the_evasion_notifications_in_the_same_corpus_read_exhausted() {
    use dereth_protocol::combat::{EvasionAttackerNotification, EvasionDefenderNotification};
    let a = bodies_of(EVASION_ATTACKER);
    let d = bodies_of(EVASION_DEFENDER);
    assert!(!a.is_empty());
    assert!(!d.is_empty());
    for b in &a {
        let (_, remaining, _) = client_read::<EvasionAttackerNotification>(&b.bytes);
        assert_eq!(
            remaining, 0,
            "{} idx {}: `0x01B3` consumes its whole body",
            b.scenario, b.idx
        );
        dereth_protocol::read_body_padded::<EvasionAttackerNotification>(&b.bytes)
            .expect("`0x01B3` must satisfy the strict gate");
    }
    for b in &d {
        let (_, remaining, _) = client_read::<EvasionDefenderNotification>(&b.bytes);
        assert_eq!(
            remaining, 0,
            "{} idx {}: `0x01B4` consumes its whole body",
            b.scenario, b.idx
        );
        dereth_protocol::read_body_padded::<EvasionDefenderNotification>(&b.bytes)
            .expect("`0x01B4` must satisfy the strict gate");
    }
}

#[test]
fn the_corpus_cannot_discriminate_the_two_widths() {
    let mut non_zero_high_dword = 0usize;
    let mut total = 0usize;
    for (op, fixed_len) in [(ATTACKER, 24usize), (DEFENDER, 28usize)] {
        for b in bodies_of(op) {
            let hi = &b.bytes[b.bytes.len() - 4..];
            if hi.iter().any(|x| *x != 0) {
                non_zero_high_dword += 1;
            }
            let name_len = usize::from(u16::from_le_bytes([b.bytes[0], b.bytes[1]]));
            let after_string = (2 + name_len).div_ceil(4) * 4;
            assert_eq!(
                after_string + fixed_len + 4,
                b.bytes.len(),
                "{} idx {}: the eight-byte reading must also consume the body exactly",
                b.scenario,
                b.idx
            );
            total += 1;
        }
    }
    assert!(total > 0);
    assert_eq!(
        non_zero_high_dword, 0,
        "a recorded body with a non-zero high dword would settle the width from the wire; \
         the corpus has none (nor do 41,767 retail bodies), so the four rests on the client's read"
    );

    let mut synth = Vec::new();
    synth.extend_from_slice(&2u16.to_le_bytes());
    synth.extend_from_slice(b"Ax");
    synth.extend_from_slice(&2u32.to_le_bytes()); // damage_type
    synth.extend_from_slice(&0.5f64.to_le_bytes()); // percent
    synth.extend_from_slice(&7u32.to_le_bytes()); // damage
    synth.extend_from_slice(&1u32.to_le_bytes()); // critical
    synth.extend_from_slice(&4u32.to_le_bytes()); // attack_conditions, low dword
    synth.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // the high dword
    let (m, remaining, _) = client_read::<AttackerNotification>(&synth);
    assert_eq!(
        m.attack_conditions,
        dereth_protocol::combat::attack_conditions::SNEAK_ATTACK
    );
    assert_eq!(
        m.attack_conditions_high, 0xDEAD_BEEF,
        "the high dword is kept apart"
    );
    assert_eq!(remaining, 0);
    assert_eq!(dereth_protocol::write_body(&m).expect("re-encodes"), synth);
}

#[test]
fn read_body_padded_accepts_every_recorded_body_now_the_codec_reads_the_high_dword() {
    let mut accepted = 0usize;
    for b in bodies_of(ATTACKER) {
        dereth_protocol::read_body_padded::<AttackerNotification>(&b.bytes)
            .unwrap_or_else(|e| panic!("{} idx {}: {e}", b.scenario, b.idx));
        let mut longer = b.bytes.clone();
        longer.extend_from_slice(&[0; 4]);
        assert!(
            dereth_protocol::read_body_padded::<AttackerNotification>(&longer).is_err(),
            "{} idx {}: a further four zero bytes are still not padding",
            b.scenario,
            b.idx
        );
        accepted += 1;
    }
    for b in bodies_of(DEFENDER) {
        dereth_protocol::read_body_padded::<DefenderNotification>(&b.bytes)
            .unwrap_or_else(|e| panic!("{} idx {}: {e}", b.scenario, b.idx));
        accepted += 1;
    }
    assert_eq!(
        accepted,
        bodies_of(ATTACKER).len() + bodies_of(DEFENDER).len()
    );
}

#[test]
fn every_recorded_attack_field_matches_its_wire_offset() {
    for b in bodies_of(ATTACKER) {
        let (m, _, _) = client_read::<AttackerNotification>(&b.bytes);
        let mut r = dereth_protocol::Reader::body(&b.bytes);
        assert_eq!(m.defender_name, r.pstring().unwrap());
        assert_eq!(m.damage_type, r.u32().unwrap());
        assert_eq!(m.percent.to_bits(), r.f64().unwrap().to_bits());
        assert_eq!(m.damage, r.u32().unwrap());
        assert_eq!(m.critical, r.u32().unwrap());
        assert_eq!(m.attack_conditions, r.u32().unwrap());
        assert_eq!(m.attack_conditions_high, r.u32().unwrap());
        r.expect_exhausted().unwrap();
    }
    for b in bodies_of(DEFENDER) {
        let (m, _, _) = client_read::<DefenderNotification>(&b.bytes);
        let mut r = dereth_protocol::Reader::body(&b.bytes);
        assert_eq!(m.attacker_name, r.pstring().unwrap());
        assert_eq!(m.damage_type, r.u32().unwrap());
        assert_eq!(m.percent.to_bits(), r.f64().unwrap().to_bits());
        assert_eq!(m.damage, r.u32().unwrap());
        assert_eq!(m.damage_location, r.u32().unwrap());
        assert_eq!(m.critical, r.u32().unwrap());
        assert_eq!(m.attack_conditions, r.u32().unwrap());
        assert_eq!(m.attack_conditions_high, r.u32().unwrap());
        r.expect_exhausted().unwrap();
    }
}
