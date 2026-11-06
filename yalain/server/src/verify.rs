//! The host's half of `dn verify <token>`: the calls to the hub's verification routes, over HTTPS
//! (HTTP to a hub on this machine), with the world's key, each on a thread of its own so the
//! world's loop never waits on the hub.
//!
//! - **The verification** goes to `POST /v1/servers/<slug>/verify` with the token and who typed
//!   it. The hub holds the call while it challenges the world's game port; the world waits for
//!   it at most `dereth_network_link::verify::VERIFY_TIMEOUT`. Its answer comes back on a later
//!   pass, as the line `yalain_federation` words it, for whoever typed it.
//! - **The answer to the hub's challenge** goes to `POST /v1/servers/<slug>/verify/answer` with
//!   the nonce, from whatever address the world's calls leave by. Nothing goes back over UDP.
//!
//! Nothing here logs the key or the token.

use std::sync::mpsc::{self, Receiver, Sender};
#[cfg(test)]
use std::time::Duration;

use dereth_network_link::verify::{send_answer, send_verification, Verification};
use yalain_federation::{ChallengeAnswer, VerifyRequest, KEY_UNKNOWN, NOT_SENT};

/// What the world's calls to the hub's verification routes name themselves.
const USER_AGENT: &str = concat!("yalain/", env!("CARGO_PKG_VERSION"));

/// The hub's answer to one verification: whom to show it to (a character of this world by id, or
/// the console when none), and the line.
pub type Answered = (Option<u64>, String);

/// The calls to the hub's verification routes, and the answers come back from them.
#[derive(Debug)]
pub struct Verifier {
    verify_url: String,
    answer_url: String,
    key: String,
    answered: (Sender<Answered>, Receiver<Answered>),
}

impl Verifier {
    /// Calls to these routes with the world's `key`.
    #[must_use]
    pub fn new(verify_url: String, answer_url: String, key: String) -> Self {
        Self {
            verify_url,
            answer_url,
            key,
            answered: mpsc::channel(),
        }
    }

    /// Sends a verification on a thread of its own; its answer comes back from
    /// [`Verifier::try_answered`]. One that cannot be sent is answered at once.
    pub fn send(&self, r: VerifyRequest) {
        let (url, key, tx) = (
            self.verify_url.clone(),
            self.key.clone(),
            self.answered.0.clone(),
        );
        let who = r.who;
        let spawned = std::thread::Builder::new()
            .name("hub-verify".into())
            .spawn(move || {
                let line = send_verification(&Verification {
                    url: &url,
                    key: &key,
                    token: &r.token,
                    by: &r.by,
                    user_agent: USER_AGENT,
                    key_unknown: KEY_UNKNOWN,
                });
                let _ = tx.send((r.who, line));
            });
        if spawned.is_err() {
            let _ = self.answered.0.send((who, NOT_SENT.to_owned()));
        }
    }

    /// Posts the answer to one of the hub's challenges on a thread of its own; what came of it is
    /// logged.
    pub fn answer(&self, a: ChallengeAnswer) {
        let (url, key) = (self.answer_url.clone(), self.key.clone());
        let spawned = std::thread::Builder::new()
            .name("hub-answer".into())
            .spawn(
                move || match send_answer(&url, &key, &a.nonce, USER_AGENT) {
                    Ok(200) => tracing::info!("Dereth Network: answered the hub's challenge"),
                    Ok(status) => tracing::info!(
                    "Dereth Network: the hub did not take the answer to its challenge ({status})"
                ),
                    Err(e) => tracing::warn!(
                        "Dereth Network: the answer to the hub's challenge could not be sent: {e}"
                    ),
                },
            );
        if spawned.is_err() {
            tracing::warn!("Dereth Network: the answer to the hub's challenge could not be sent");
        }
    }

    /// A verification the hub has answered, if one has.
    pub fn try_answered(&self) -> Option<Answered> {
        self.answered.1.try_recv().ok()
    }

    /// Waits until the hub has answered a verification, for at most `guard` (a guard against a
    /// hang), leaving the answer for [`Verifier::try_answered`]: whether one came.
    #[cfg(test)]
    pub fn wait_for_answer(&self, guard: Duration) -> bool {
        match self.answered.1.recv_timeout(guard) {
            Ok(a) => self.answered.0.send(a).is_ok(),
            Err(_) => false,
        }
    }
}
