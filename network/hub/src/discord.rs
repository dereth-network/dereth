//! Signing world operators in with Discord: the OAuth 2 authorisation-code flow, `identify` only.
//!
//! The hub sends the browser to Discord's authorisation page, Discord sends it back with a code,
//! and the hub exchanges the code (with the application's client secret) for a token it uses once,
//! to read who signed in. It keeps the user's id and names, never the token.

use std::time::Duration;

use serde_json::Value;

use crate::fetch::{self, Fetch, Url};

/// Discord's own addresses.
pub const AUTHORIZE: &str = "https://discord.com/oauth2/authorize";
/// See [`AUTHORIZE`].
pub const API: &str = "https://discord.com/api/v10";

/// How long the hub asks Discord nothing after a `429` that names no wait, in seconds.
pub const DEFAULT_WAIT_SECS: u64 = 60;

/// The longest wait a `429` is taken at, in seconds: a longer one is waited this long, and the
/// next sign-in asks again.
pub const MAX_WAIT_SECS: u64 = 3_600;

/// The Discord application the hub signs operators in with.
#[derive(Clone)]
pub struct Discord {
    pub client_id: String,
    /// Read from the host's environment or a file the hub's operator names; never logged.
    pub client_secret: String,
    /// Discord's authorisation page ([`AUTHORIZE`]; a stand-in in tests and development).
    pub authorize: String,
    /// Discord's API ([`API`]; likewise).
    pub api: String,
    /// Which addresses the exchange may connect to: any, for Discord itself.
    pub rule: fetch::AddressRule,
}

impl std::fmt::Debug for Discord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Discord")
            .field("client_id", &self.client_id)
            .field("authorize", &self.authorize)
            .field("api", &self.api)
            .finish_non_exhaustive()
    }
}

/// Why Discord gave no user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscordError {
    /// Discord answered `429`: it asks for nothing more for this many seconds (its `Retry-After`,
    /// within [`MAX_WAIT_SECS`]).
    Wait(u64),
    /// Anything else: Discord refusing the code, an answer that is not one, Discord unreachable.
    Failed(String),
}

impl std::fmt::Display for DiscordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wait(secs) => write!(f, "Discord asks for nothing for {secs} seconds"),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

/// The wait a `429` asks for, in whole seconds: its `Retry-After` header, or else its body's
/// `retry_after`, rounded up, at least a second and at most [`MAX_WAIT_SECS`];
/// [`DEFAULT_WAIT_SECS`] when it names neither.
#[must_use]
pub fn wait_of(headers: &[(String, String)], body: &[u8]) -> u64 {
    let header = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
        .and_then(|(_, v)| v.trim().parse::<f64>().ok());
    let body = || {
        serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|v| v["retry_after"].as_f64())
    };
    match header.or_else(body) {
        Some(secs) if secs.is_finite() && secs >= 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = secs.ceil().min(MAX_WAIT_SECS as f64) as u64;
            whole.clamp(1, MAX_WAIT_SECS)
        }
        _ => DEFAULT_WAIT_SECS,
    }
}

/// Who signed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordUser {
    pub id: String,
    /// The display name, or the user name where there is none.
    pub name: String,
}

/// Any address will do: Discord's addresses are the hub operator's configuration, not a user's.
#[must_use]
pub fn any_address(_: std::net::IpAddr) -> bool {
    true
}

/// A secret of the Discord application's: `from_env`, the value of its environment variable, when
/// that holds anything but white space, or else the contents of `file`; trimmed either way. None
/// when neither is given, or when they hold nothing but white space. The error names the file and
/// why it could not be read, never what it holds.
///
/// # Errors
/// When `file` cannot be read.
pub fn secret_of(from_env: Option<String>, file: Option<&str>) -> Result<Option<String>, String> {
    if let Some(s) = from_env.filter(|s| !s.trim().is_empty()) {
        return Ok(Some(s.trim().to_owned()));
    }
    let Some(path) = file else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let text = text.trim();
    Ok((!text.is_empty()).then(|| text.to_owned()))
}

/// `s` written for a URL's query or a form body.
#[must_use]
pub fn form_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl Discord {
    /// Discord's application with the client secret from the environment variable
    /// `DERETH_HUB_DISCORD_CLIENT_SECRET`, or else the file `secret_file`.
    ///
    /// # Errors
    /// When there is no secret.
    pub fn new(client_id: String, secret_file: Option<&str>) -> Result<Self, String> {
        let secret = secret_of(
            std::env::var("DERETH_HUB_DISCORD_CLIENT_SECRET").ok(),
            secret_file,
        )?
        .unwrap_or_default();
        if secret.is_empty() {
            return Err("Discord sign-in needs the client secret: set DERETH_HUB_DISCORD_CLIENT_SECRET in the hub's environment, or name a file with --discord-client-secret-file".into());
        }
        Ok(Self {
            client_id,
            client_secret: secret,
            authorize: AUTHORIZE.into(),
            api: API.into(),
            rule: any_address,
        })
    }

    /// The authorisation page's address for one sign-in.
    #[must_use]
    pub fn authorize_url(&self, state: &str, redirect_uri: &str) -> String {
        format!(
            "{}?response_type=code&client_id={}&scope=identify&state={}&redirect_uri={}&prompt=none",
            self.authorize,
            form_encode(&self.client_id),
            form_encode(state),
            form_encode(redirect_uri),
        )
    }

    fn request(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<Value, DiscordError> {
        let url = Url::parse(url).map_err(DiscordError::Failed)?;
        let r = Fetch {
            method,
            url,
            headers,
            body,
            timeout: Duration::from_secs(10),
            max_body: 64 * 1024,
            max_redirects: 0,
            rule: self.rule,
            deny: Vec::new(),
            check: fetch::any_url,
            check_with: Vec::new(),
        }
        .send()
        .map_err(|e| DiscordError::Failed(format!("Discord: {e}")))?;
        if r.status == 429 {
            return Err(DiscordError::Wait(wait_of(&r.headers, &r.body)));
        }
        if r.status != 200 {
            return Err(DiscordError::Failed(format!(
                "Discord answered {}",
                r.status
            )));
        }
        serde_json::from_slice(&r.body)
            .map_err(|_| DiscordError::Failed("Discord's answer is not JSON".to_owned()))
    }

    /// Exchanges an authorisation code for the user who signed in.
    ///
    /// # Errors
    /// Discord refusing the code, asking the hub to wait, or being unreachable.
    pub fn user_for_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<DiscordUser, DiscordError> {
        let form = format!(
            "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&client_secret={}",
            form_encode(code),
            form_encode(redirect_uri),
            form_encode(&self.client_id),
            form_encode(&self.client_secret),
        );
        let token = self.request(
            "POST",
            &format!("{}/oauth2/token", self.api),
            vec![
                (
                    "Content-Type".into(),
                    "application/x-www-form-urlencoded".into(),
                ),
                ("Accept".into(), "application/json".into()),
            ],
            form.into_bytes(),
        )?;
        let access = token["access_token"]
            .as_str()
            .filter(|t| !t.is_empty() && t.len() < 512)
            .ok_or_else(|| DiscordError::Failed("Discord gave no token".to_owned()))?;
        let me = self.request(
            "GET",
            &format!("{}/users/@me", self.api),
            vec![
                ("Authorization".into(), format!("Bearer {access}")),
                ("Accept".into(), "application/json".into()),
            ],
            Vec::new(),
        )?;
        user_of(&me).map_err(|e| DiscordError::Failed(e.to_owned()))
    }
}

/// The user Discord's `users/@me` answer names: its id, and its display name without hidden
/// characters (its id when that leaves nothing).
fn user_of(me: &serde_json::Value) -> Result<DiscordUser, &'static str> {
    let id = me["id"]
        .as_str()
        .filter(|i| (1..=24).contains(&i.len()) && i.bytes().all(|b| b.is_ascii_digit()))
        .ok_or("Discord gave no user id")?
        .to_owned();
    let name = me["global_name"]
        .as_str()
        .or_else(|| me["username"].as_str())
        .unwrap_or("");
    let name = crate::text::without_hidden(name)
        .chars()
        .take(64)
        .collect::<String>();
    Ok(DiscordUser {
        name: if name.trim().is_empty() {
            id.clone()
        } else {
            name
        },
        id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: hub: an operator's name is its Discord display name without control, bidirectional
    /// or zero-width characters, and its id when that leaves nothing.
    #[test]
    fn an_operators_name_from_discord_loses_its_hidden_characters() {
        let user = |v: serde_json::Value| user_of(&v).expect("a user");
        let u = user(
            serde_json::json!({ "id": "4012", "global_name": "Ash\u{202e}er\u{200b}on\u{7}" }),
        );
        assert_eq!((u.id.as_str(), u.name.as_str()), ("4012", "Asheron"));
        assert_eq!(
            user(serde_json::json!({ "id": "4012", "username": "\u{200b}\u{202e}" })).name,
            "4012"
        );
        assert!(user_of(&serde_json::json!({ "id": "x1" })).is_err());
    }

    /// Spec: hub: a `429` from Discord is waited out for its `Retry-After`, or its body's
    /// `retry_after`, rounded up, at least a second and at most an hour; a minute when it names
    /// neither.
    #[test]
    fn a_429_from_discord_is_waited_out_for_the_time_it_names() {
        let h = |v: &str| vec![("Retry-After".to_owned(), v.to_owned())];
        assert_eq!(wait_of(&h("30"), b""), 30);
        assert_eq!(wait_of(&h("1.2"), b""), 2);
        assert_eq!(wait_of(&h("0"), b""), 1);
        assert_eq!(wait_of(&h("86400"), b""), MAX_WAIT_SECS);
        assert_eq!(MAX_WAIT_SECS, 3_600);
        assert_eq!(
            wait_of(&[("retry-after".to_owned(), "7".to_owned())], b""),
            7,
            "any case"
        );
        assert_eq!(
            wait_of(
                &[],
                br#"{"message":"You are being rate limited.","retry_after":4.5,"global":false}"#
            ),
            5
        );
        assert_eq!(
            wait_of(&h("12"), br#"{"retry_after":99}"#),
            12,
            "the header first"
        );
        assert_eq!(wait_of(&h("soon"), b"not json"), DEFAULT_WAIT_SECS);
        assert_eq!(wait_of(&[], b""), DEFAULT_WAIT_SECS);
        assert_eq!(DEFAULT_WAIT_SECS, 60);
        assert_eq!(wait_of(&h("-5"), b""), DEFAULT_WAIT_SECS);
    }

    /// Spec: hub: sign-in asks Discord for `identify` only, with the hub's state and redirect.
    #[test]
    fn the_authorisation_page_is_asked_for_identify_with_the_state() {
        let d = Discord {
            client_id: "123".into(),
            client_secret: "sekrit-value".into(),
            authorize: AUTHORIZE.into(),
            api: API.into(),
            rule: any_address,
        };
        let u = d.authorize_url(
            "st+/=",
            "https://hub.dereth.network/v1/auth/discord/callback",
        );
        assert!(u.starts_with("https://discord.com/oauth2/authorize?response_type=code&client_id=123&scope=identify&state=st%2B%2F%3D&redirect_uri=https%3A%2F%2Fhub.dereth.network%2Fv1%2Fauth%2Fdiscord%2Fcallback"), "{u}");
        assert!(
            !format!("{d:?}").contains("sekrit"),
            "the secret is never shown"
        );
    }
}
