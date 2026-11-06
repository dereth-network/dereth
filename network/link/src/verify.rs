//! A world's calls to the hub's verification routes, over HTTPS (HTTP to a hub on this machine),
//! with the world's key. Each call blocks, so a world makes it on a thread of its own, and nothing
//! here logs the key or the token.
//!
//! **The verification** goes to `POST /v1/servers/<slug>/verify` with the token and who typed it;
//! the hub holds the call while it challenges the world's game port. **The answer to the hub's
//! challenge** goes to `POST /v1/servers/<slug>/verify/answer` with the nonce, from whatever address
//! the world's calls leave by.

use std::time::Duration;

use dereth_network_protocol::verification::{answer_line, unreachable_line};

/// How long a verification waits for the hub: it holds the call while it looks the world's host up
/// and waits for the answer to its challenge.
pub const VERIFY_TIMEOUT: Duration = Duration::from_secs(60);

/// The most of the hub's answer read.
const MAX_BODY: u64 = 64 * 1024;

/// The agent a world's calls to these routes go through, naming itself `user_agent`: no redirects,
/// and the hub's status taken as it is.
fn agent(user_agent: &str) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(user_agent)
        .timeout_global(Some(VERIFY_TIMEOUT))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into()
}

/// One verification, for [`send_verification`].
#[derive(Debug, Clone, Copy)]
pub struct Verification<'a> {
    /// The hub's verification route.
    pub url: &'a str,
    /// The world's key.
    pub key: &'a str,
    /// The token the hub gave the world's operator.
    pub token: &'a str,
    /// Who typed it, for the hub's log.
    pub by: &'a str,
    /// What the call names itself.
    pub user_agent: &'a str,
    /// The line a refusal of the world's key is shown as, naming where this server sets it.
    pub key_unknown: &'a str,
}

/// Sends one verification to the hub and says how it went, as the line to show.
#[must_use]
pub fn send_verification(v: &Verification<'_>) -> String {
    let body = serde_json::json!({ "token": v.token, "by": v.by }).to_string();
    match agent(v.user_agent)
        .post(v.url)
        .header("Authorization", &format!("Bearer {}", v.key))
        .header("Content-Type", "application/json")
        .send(body.as_str())
    {
        Ok(mut resp) => {
            let status = resp.status().as_u16();
            let text = resp
                .body_mut()
                .with_config()
                .limit(MAX_BODY)
                .read_to_string()
                .unwrap_or_default();
            answer_line(status, &text, v.key_unknown)
        }
        Err(e) => unreachable_line(&e.to_string()),
    }
}

/// Posts a challenge's nonce (32 hex digits) to the hub's answer route with the world's key: the
/// hub's status.
///
/// # Errors
/// The hub could not be reached.
pub fn send_answer(url: &str, key: &str, nonce: &str, user_agent: &str) -> Result<u16, String> {
    let body = serde_json::json!({ "nonce": nonce }).to_string();
    agent(user_agent)
        .post(url)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send(body.as_str())
        .map(|r| r.status().as_u16())
        .map_err(|e| e.to_string())
}
