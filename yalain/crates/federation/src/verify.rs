//! `dn verify <token>`: the operator proves to the hub that this world is the one registered or
//! claimed on the site, at the console or (an admin's character) in game, as Empyrean's
//! command does (`docs/network/03-world-behaviour.md` section 6, "Verifying in game";
//! `05-hub.md` section 2.5).
//!
//! - **The request.** The world asks the host to send the hub the token with the world's key
//!   ([`Federation::take_verify_requests`]); the host sends it off the world's loop, and hands the
//!   hub's answer back as a line for whoever typed it ([`Federation::verify_answered`]). One
//!   verification goes at a time: a second before the hub answers is refused.
//! - **The challenge.** Meanwhile the hub sends the world's game port its challenge. While a
//!   verification waits for the hub, and for [`WINDOW_SECS`] at most from when it was typed
//!   ([`Federation::hearing_challenges`]), the host hands the world each challenge its game port
//!   heard ([`Federation::heard_challenges`]); one for this world's own slug is answered once for
//!   each nonce, and for at most [`MAX_ANSWERS`] nonces ([`Federation::take_challenge_answers`]),
//!   over HTTPS by the host. Nothing is ever answered over UDP.
//! - **The answer.** The hub's answer to the verification is one line ([`answer_line`]), or the
//!   line that it could not be reached ([`unreachable_line`]).

use dereth_network_protocol::challenge::{
    nonce_hex, Challenge, MAX_ANSWERS, NONCE_LEN, WINDOW_SECS,
};

use crate::{Federation, WORLD_OFF};

/// What the operator is shown as the verification goes to the hub.
pub const ASKING: &str = "Asking the Dereth Network's hub to verify this world...";

/// What a second verification is answered with while one waits for the hub.
pub const ON_ITS_WAY: &str = "A verification is on its way to the hub already.";

/// The command and what it does, as its help line has it (`dn` is `@dn` in game).
pub const VERIFY_HELP: &str =
    "dn verify <token> - verify this world with the token the site gave you";

/// The longest name of whoever typed it the hub's log takes.
pub const MAX_BY_CHARS: usize = 32;

pub use dereth_network_protocol::verification::{unreachable_line, MAX_MESSAGE_CHARS, NOT_SENT};

/// What a refusal of the world's key is answered with, and where the key is set.
pub const KEY_UNKNOWN: &str = "The Dereth Network did not verify this world: the hub does not \
                               know this world's key. Check federation.key_file (--hub-key-file) \
                               against the key the site gave you.";

/// A `dn verify` for the host to send to the hub's verification route, with the world's key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyRequest {
    /// The token the hub gave the world's operator.
    pub token: String,
    /// Who typed it, for the hub's log.
    pub by: String,
    /// Whom to show the hub's answer to: a character of this world (by id), or the console.
    pub who: Option<u64>,
}

/// The answer to a challenge the hub sent this world's game port while a verification waits: the
/// host posts its nonce to the hub's answer route over HTTPS, with the world's key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeAnswer {
    /// The challenge's nonce, as 32 hex digits.
    pub nonce: String,
}

/// The verification waiting for the hub, if any, and what it has answered.
#[derive(Debug, Default)]
pub(crate) struct Verification {
    /// When the one waiting was typed; none while none waits.
    asked_at: Option<f64>,
    /// The nonces answered for the one waiting.
    answered: Vec<[u8; NONCE_LEN]>,
    requests: Vec<VerifyRequest>,
    answers: Vec<ChallengeAnswer>,
}

/// Whether `token` has the shape of the hub's verification token: `dnv_` and 32 characters of
/// base64url.
#[must_use]
pub fn is_verification_token(token: &str) -> bool {
    token.len() == 36
        && token.starts_with("dnv_")
        && token[4..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The line the hub's answer to a verification (its status and its JSON body) is shown as.
#[must_use]
pub fn answer_line(status: u16, body: &str) -> String {
    dereth_network_protocol::verification::answer_line(status, body, KEY_UNKNOWN)
}

impl Federation {
    /// `dn verify <token>` (its words after `verify`), typed at `t` by `by`: the line to answer it
    /// with at once. `who` is the character to show the hub's answer to, or the console when none
    /// (whose help line is written `dn`, a character's `@dn`).
    pub fn verify(&mut self, t: f64, who: Option<u64>, by: &str, args: &[&str]) -> String {
        if !self.enabled() {
            return WORLD_OFF.to_owned();
        }
        let token = match args {
            [token] if is_verification_token(token.trim()) => token.trim().to_owned(),
            _ if who.is_some() => return format!("@{VERIFY_HELP}"),
            _ => return VERIFY_HELP.to_owned(),
        };
        let v = &mut self.verification;
        if v.asked_at.is_some() {
            return ON_ITS_WAY.to_owned();
        }
        v.asked_at = Some(t);
        v.answered.clear();
        v.requests.push(VerifyRequest {
            token,
            by: by.chars().take(MAX_BY_CHARS).collect(),
            who,
        });
        ASKING.to_owned()
    }

    /// The verifications for the host to send, oldest first.
    pub fn take_verify_requests(&mut self) -> Vec<VerifyRequest> {
        std::mem::take(&mut self.verification.requests)
    }

    /// Whether the world reads the hub's challenges at `t`: a verification waits for the hub, and
    /// was typed no more than [`WINDOW_SECS`] before. Otherwise its game port drops them, as it
    /// does with the network off.
    #[must_use]
    pub fn hearing_challenges(&self, t: f64) -> bool {
        self.enabled()
            && self
                .verification
                .asked_at
                .is_some_and(|at| t - at <= WINDOW_SECS)
    }

    /// The hub's challenges the game port heard, at `t`. Each is answered only while
    /// [`Federation::hearing_challenges`], for this world's own slug, once for each nonce and for
    /// at most [`MAX_ANSWERS`] nonces; any other is ignored.
    pub fn heard_challenges(&mut self, t: f64, heard: impl IntoIterator<Item = Challenge>) {
        let hearing = self.hearing_challenges(t);
        let slug = &self.settings.slug;
        let v = &mut self.verification;
        for c in heard {
            if !hearing
                || c.slug != *slug
                || v.answered.contains(&c.nonce)
                || v.answered.len() >= MAX_ANSWERS
            {
                continue;
            }
            v.answered.push(c.nonce);
            v.answers.push(ChallengeAnswer {
                nonce: nonce_hex(&c.nonce),
            });
        }
    }

    /// The answers to the hub's challenges for the host to post, oldest first.
    pub fn take_challenge_answers(&mut self) -> Vec<ChallengeAnswer> {
        std::mem::take(&mut self.verification.answers)
    }

    /// The hub answered the verification waiting (or it could not be sent): another may be typed,
    /// and no challenge is answered until it is.
    pub fn verify_answered(&mut self) {
        self.verification.asked_at = None;
    }
}
