//! Checking a world from outside: the Dereth Network's hub tries the host's own address at the
//! world's game port, as a friend elsewhere would, and says whether anything answered.
//!
//! The launcher sends `POST /v1/probe` with `{"port": <the game port>}` to the hub; the hub tries
//! the address the call came from (never one the launcher names), with Empyrean's status ping and
//! the server-tracker login, and answers `{"address", "port", "reached", "world"}`: the address and
//! port it tried, whether anything answered, and what the world said of itself (its name and
//! players) when it said anything. The hub limits how often one address may ask.
//!
//! This module builds the call and reads the answer; sending it is the launcher's.

use serde::{Deserialize, Serialize};

/// The hub the launcher asks, unless a list of another hub's is read instead.
pub const HUB: &str = "https://hub.dereth.network";

/// The hub's route.
pub const PATH: &str = "/v1/probe";

/// The hub to ask: the one whose list `hub_list` names (`DERETH_HUB_LIST`, a `.../v1/servers`
/// URL), else the network's own.
#[must_use]
pub fn hub_base(hub_list: Option<&str>) -> String {
    hub_list
        .map(str::trim)
        .and_then(|l| l.split('?').next())
        .and_then(|l| l.strip_suffix("/v1/servers"))
        .filter(|b| b.starts_with("https://") || b.starts_with("http://"))
        .map_or_else(|| HUB.to_owned(), str::to_owned)
}

/// The call's body.
#[must_use]
pub fn body(port: u16) -> String {
    serde_json::json!({ "port": port }).to_string()
}

/// What the world said of itself when the hub tried it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub players: Option<u32>,
}

/// The hub's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    /// The address the hub tried: this computer's on the internet.
    pub address: String,
    pub port: u16,
    /// Whether anything answered there.
    pub reached: bool,
    #[serde(default)]
    pub world: Option<World>,
}

/// What came of asking the hub, as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    /// The hub tried and said what it found.
    Answered(Answer),
    /// The hub limits how often one address asks: try again in so many seconds.
    TooSoon { seconds: u64 },
    /// The hub would not try this address (one it would never reach on the internet).
    Refused { message: String },
    /// The hub could not be asked, or said nothing that reads.
    Failed { message: String },
}

/// Reads the hub's answer: its status and its body, and `Retry-After` when it gave one.
#[must_use]
pub fn read(status: u16, body: &str, retry_after: Option<&str>) -> Outcome {
    let value: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let message = value
        .pointer("/error/message")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    match status {
        200 => serde_json::from_value::<Answer>(value).map_or_else(
            |_| Outcome::Failed {
                message: "The hub's answer did not read.".into(),
            },
            Outcome::Answered,
        ),
        429 => Outcome::TooSoon {
            seconds: retry_after
                .and_then(|s| s.trim().parse().ok())
                .or_else(|| {
                    value
                        .pointer("/error/retryAfterSeconds")
                        .and_then(serde_json::Value::as_u64)
                })
                .unwrap_or(60),
        },
        400 | 403 => Outcome::Refused {
            message: message.unwrap_or_else(|| "The hub would not try this world.".into()),
        },
        _ => Outcome::Failed {
            message: message.unwrap_or_else(|| format!("The hub answered {status}.")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_network_hub_is_asked_unless_another_hubs_list_is_read() {
        assert_eq!(hub_base(None), "https://hub.dereth.network");
        assert_eq!(
            hub_base(Some("http://127.0.0.1:8650/v1/servers")),
            "http://127.0.0.1:8650"
        );
        assert_eq!(
            hub_base(Some("https://hub.example.org/v1/servers?limit=100")),
            "https://hub.example.org"
        );
        assert_eq!(
            hub_base(Some("file:///lists/servers.json")),
            "https://hub.dereth.network",
            "a list that is no hub's"
        );
        assert_eq!(body(9000), r#"{"port":9000}"#);
    }

    #[test]
    fn the_hubs_answer_says_whether_the_world_was_reached_and_what_it_said() {
        let reached = read(
            200,
            r#"{"address":"203.0.113.7","port":9000,"reached":true,"world":{"name":"My world","players":2,"software":"Empyrean","version":"0.3.3","state":"open"}}"#,
            None,
        );
        assert_eq!(
            reached,
            Outcome::Answered(Answer {
                address: "203.0.113.7".into(),
                port: 9000,
                reached: true,
                world: Some(World {
                    name: Some("My world".into()),
                    players: Some(2),
                }),
            })
        );
        let silent = read(
            200,
            r#"{"address":"203.0.113.7","port":9000,"reached":false,"world":null}"#,
            None,
        );
        assert!(matches!(
            silent,
            Outcome::Answered(Answer {
                reached: false,
                world: None,
                ..
            })
        ));
    }

    #[test]
    fn a_refusal_a_limit_and_a_failure_are_each_said_plainly() {
        assert_eq!(
            read(
                429,
                r#"{"error":{"code":"RATE_LIMITED","message":"Too many requests","retryAfterSeconds":700}}"#,
                Some("720")
            ),
            Outcome::TooSoon { seconds: 720 }
        );
        assert_eq!(
            read(429, r#"{"error":{"retryAfterSeconds":700}}"#, None),
            Outcome::TooSoon { seconds: 700 }
        );
        assert_eq!(
            read(
                403,
                r#"{"error":{"code":"FORBIDDEN","message":"Not on the internet"}}"#,
                None
            ),
            Outcome::Refused {
                message: "Not on the internet".into()
            }
        );
        assert!(matches!(
            read(200, "nonsense", None),
            Outcome::Failed { .. }
        ));
        assert!(matches!(read(502, "", None), Outcome::Failed { .. }));
    }
}
