//! The operator's `dn verify <token>`: one verification at a time, the hub's challenge answered
//! while it waits, and the line each of the hub's answers is shown as.

use dereth_network_protocol::challenge::{
    nonce_hex, Challenge, MAX_ANSWERS, NONCE_LEN, WINDOW_SECS,
};
use yalain_federation::{
    answer_line, unreachable_line, ChallengeAnswer, VerifyRequest, ASKING, HELP, KEY_UNKNOWN,
    ON_ITS_WAY, VERIFY_HELP,
};

use crate::{World, SLUG};

/// A token of the shape the hub gives.
const TOKEN: &str = "dnv_abcdefghijklmnopqrstuvwxyzAB-_01";

fn challenge(slug: &str, nonce: u8) -> Challenge {
    Challenge {
        slug: slug.to_owned(),
        nonce: [nonce; NONCE_LEN],
    }
}

/// The nonces the world posts back for these challenges heard now.
fn answered(w: &mut World, heard: &[Challenge]) -> Vec<String> {
    w.fed.heard_challenges(w.t, heard.iter().cloned());
    w.fed
        .take_challenge_answers()
        .into_iter()
        .map(|ChallengeAnswer { nonce }| nonce)
        .collect()
}

/// `dn verify <token>` at the console (its word in any case) asks the host to send the hub the
/// token, as the console's, and says so; a token of the wrong shape, or none, is answered with the
/// command's help line,
/// which `dn help` lists; with the network off it says the world is not on it, and asks nothing.
/// Behaviour: server.network.verify-at-the-console
#[test]
fn dn_verify_at_the_console_asks_the_hub_and_says_so() {
    let mut off = World::with(false);
    assert_eq!(
        off.operator(&["verify", TOKEN]),
        ["This world is not on the Dereth Network."]
    );
    assert!(off.fed.take_verify_requests().is_empty());

    let mut w = World::new();
    assert!(w.operator(&["help"]).contains(&VERIFY_HELP.to_owned()));
    assert!(HELP.contains(&VERIFY_HELP));
    for wrong in [
        &["verify"][..],
        &["verify", "dnv_short"],
        &["verify", TOKEN, "more"],
    ] {
        assert_eq!(w.operator(wrong), [VERIFY_HELP], "{wrong:?}");
    }
    assert!(w.fed.take_verify_requests().is_empty());
    assert_eq!(w.operator(&["Verify", TOKEN]), [ASKING]);
    assert_eq!(
        w.fed.take_verify_requests(),
        [VerifyRequest {
            token: TOKEN.into(),
            by: "the console".into(),
            who: None,
        }]
    );
}

/// One verification goes at a time: a second, at the console or in game, while the first waits
/// for the hub, is answered that one is on its way and sends nothing; once the hub answered, the
/// next goes. In game the help line is written with `@`, and the name of whoever typed it is cut
/// to 32 characters for the hub's log.
/// Behaviour: server.network.verify-one-at-a-time
#[test]
fn a_second_verification_while_one_waits_is_refused_and_sends_nothing() {
    let mut w = World::new();
    let t = w.t;
    assert_eq!(
        w.fed.verify(t, Some(7), "Asheron", &["nope"]),
        format!("@{VERIFY_HELP}")
    );
    let long = "A".repeat(40);
    assert_eq!(w.fed.verify(t, Some(7), &long, &[TOKEN]), ASKING);
    assert_eq!(
        w.fed.take_verify_requests(),
        [VerifyRequest {
            token: TOKEN.into(),
            by: "A".repeat(32),
            who: Some(7),
        }]
    );
    assert_eq!(w.operator(&["verify", TOKEN]), [ON_ITS_WAY]);
    assert_eq!(w.fed.verify(t + 5.0, Some(8), "Kara", &[TOKEN]), ON_ITS_WAY);
    assert!(w.fed.take_verify_requests().is_empty(), "nothing more goes");
    w.fed.verify_answered();
    assert_eq!(w.operator(&["verify", TOKEN]), [ASKING]);
    assert_eq!(w.fed.take_verify_requests().len(), 1);
}

/// While a verification waits, and for a minute at most from when it was typed, the world reads
/// the hub's challenge for its own slug and posts each nonce back once, as 32 hex digits; a
/// challenge for another slug, one before any verification, one past the minute and one after
/// the hub answered are ignored.
/// Behaviour: server.network.verify-challenge-answered-once
#[test]
fn a_challenge_for_this_world_is_answered_once_while_its_verification_waits() {
    let mut w = World::new();
    assert!(!w.fed.hearing_challenges(w.t));
    assert!(
        answered(&mut w, &[challenge(SLUG, 1)]).is_empty(),
        "none waits"
    );

    w.operator(&["verify", TOKEN]);
    assert!(w.fed.hearing_challenges(w.t));
    assert_eq!(
        answered(&mut w, &[challenge(SLUG, 1), challenge("coldeve", 2)]),
        [nonce_hex(&[1; NONCE_LEN])],
        "its own slug alone"
    );
    assert!(
        answered(&mut w, &[challenge(SLUG, 1)]).is_empty(),
        "each nonce once"
    );
    w.advance(WINDOW_SECS);
    assert_eq!(
        answered(&mut w, &[challenge(SLUG, 3)]),
        [nonce_hex(&[3; NONCE_LEN])],
        "within the minute"
    );
    w.advance(0.5);
    assert!(!w.fed.hearing_challenges(w.t));
    assert!(
        answered(&mut w, &[challenge(SLUG, 4)]).is_empty(),
        "past the minute"
    );

    w.fed.verify_answered();
    w.operator(&["verify", TOKEN]);
    w.fed.verify_answered();
    assert!(!w.fed.hearing_challenges(w.t));
    assert!(
        answered(&mut w, &[challenge(SLUG, 5)]).is_empty(),
        "the hub answered"
    );
    let off = World::with(false);
    assert!(!off.fed.hearing_challenges(off.t));
}

/// However many challenges arrive, at most eight nonces are posted back for one verification; a
/// new verification answers afresh.
/// Behaviour: server.network.verify-eight-nonces
#[test]
fn at_most_eight_nonces_are_answered_for_one_verification() {
    let mut w = World::new();
    w.operator(&["verify", TOKEN]);
    let heard: Vec<Challenge> = (0..12).map(|n| challenge(SLUG, n)).collect();
    let posted = answered(&mut w, &heard);
    assert_eq!(MAX_ANSWERS, 8);
    assert_eq!(
        posted,
        (0..8)
            .map(|n| nonce_hex(&[n; NONCE_LEN]))
            .collect::<Vec<_>>()
    );
    assert!(answered(&mut w, &[challenge(SLUG, 20)]).is_empty());
    w.fed.verify_answered();
    w.operator(&["verify", TOKEN]);
    assert_eq!(answered(&mut w, &heard[..1]).len(), 1, "afresh");
}

/// Each of the hub's answers is one line, in the network's own words: verified with each
/// approval, the key unknown (and where it is set), too many attempts, any other refusal with
/// the hub's own message (without its control characters, at most 200 characters, ended as a
/// sentence), and a hub that could not be reached.
/// Behaviour: server.network.verify-answer-lines
#[test]
fn each_of_the_hubs_answers_is_one_line_in_the_networks_words() {
    let verified = |approval: &str| {
        answer_line(
            200,
            &format!(r#"{{"verified":true,"federation":"on","approval":"{approval}"}}"#),
        )
    };
    assert_eq!(
        verified("approved"),
        "The Dereth Network verified this world. It is on the network now; the link comes up \
         within a minute."
    );
    assert_eq!(
        verified("rate_limited"),
        "The Dereth Network verified this world. Today's allowance of new worlds is spent: turn \
         federation on from the site later."
    );
    assert_eq!(
        verified("not_network_slug"),
        "The Dereth Network verified this world. Its slug is too long, or reserved, to join the \
         network."
    );
    assert_eq!(
        verified("locked"),
        "The Dereth Network verified this world, but the hub's operator has locked it."
    );
    assert_eq!(
        answer_line(401, r#"{"error":{"code":"unauthorized","message":"no"}}"#),
        KEY_UNKNOWN
    );
    assert!(KEY_UNKNOWN.starts_with(
        "The Dereth Network did not verify this world: the hub does not know this world's key."
    ));
    assert!(KEY_UNKNOWN.contains("federation.key_file"));
    assert_eq!(
        answer_line(429, "{}"),
        "The Dereth Network did not verify this world: too many attempts. Wait a while and try \
         again."
    );
    let hubs = "The hub's challenge to retail.dereth.network:19000 (99.113.32.94) got no answer: \
                is the game port reachable from the internet?";
    assert_eq!(
        answer_line(
            422,
            &format!(r#"{{"error":{{"code":"no_answer","message":"{hubs}"}}}}"#)
        ),
        format!("The Dereth Network did not verify this world: {hubs}")
    );
    assert_eq!(
        answer_line(
            409,
            r#"{"error":{"code":"in_progress","message":"Another verification\u0007 waits"}}"#
        ),
        "The Dereth Network did not verify this world: Another verification waits."
    );
    let long = "x".repeat(300);
    assert_eq!(
        answer_line(
            422,
            &format!(r#"{{"error":{{"code":"x","message":"{long}"}}}}"#)
        ),
        format!(
            "The Dereth Network did not verify this world: {}.",
            "x".repeat(200)
        )
    );
    assert_eq!(
        answer_line(503, "not json"),
        "The Dereth Network did not verify this world: the hub answered 503."
    );
    assert_eq!(
        unreachable_line("connection refused"),
        "The Dereth Network's hub could not be reached: connection refused."
    );
}
