//! Reach: the question the hub asks a world before it carries a tell to one of the world's
//! characters elsewhere, while the character is not online, and the world's answer
//! (`docs/network/02-link-protocol.md` section 8, `05-hub.md` section 5.8).
//!
//! A tell to a character that is not online is answered `offline`, by its world. A hub that can
//! carry the tell to the character's person (to its Discord account) first asks the world whether
//! the character, not online, would have been shown the tell: its squelches and its operator's
//! blocks are the world's to keep, and the world's to apply.
//!
//! The reach messages are a family of their own: [`ReachMessage`] reads them, and
//! [`crate::Message`] skips them as types it does not know, as every reader of a link skips a type
//! it does not know. A world that knows nothing of them never answers, and the hub carries the
//! tell nowhere else.

use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::wire::{WireError, MAX_ID_CHARS, MAX_MESSAGE_BYTES};

/// One of the reach messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ReachMessage {
    /// Hub to world: a tell from `from` to `to`, one of this world's characters, was answered
    /// `offline`. Would `to`, not online, have been shown it?
    #[serde(rename = "reach")]
    Ask {
        id: String,
        from: Address,
        to: Address,
    },
    /// World to hub: `reachable` when the character is not online and would have been shown the
    /// tell (on the network, squelching neither the sender nor its world, and blocking neither);
    /// false for anything else, alike.
    #[serde(rename = "reach.result")]
    Result { id: String, reachable: bool },
}

/// Every `type` of the reach messages.
pub const TYPES: &[&str] = &["reach", "reach.result"];

impl ReachMessage {
    /// Reads one frame.
    ///
    /// # Errors
    /// [`WireError::Unknown`] for a type that is not one of the reach messages,
    /// [`WireError::Malformed`] for anything else that is not one of them.
    pub fn from_json(text: &str) -> Result<Self, WireError> {
        if text.len() > MAX_MESSAGE_BYTES {
            return Err(WireError::Malformed("longer than the limit".into()));
        }
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| WireError::Malformed(e.to_string()))?;
        let Some(kind) = value.get("type").and_then(serde_json::Value::as_str) else {
            return Err(WireError::Malformed("no type".into()));
        };
        if !TYPES.contains(&kind) {
            return Err(WireError::Unknown(kind.to_owned()));
        }
        let kind = kind.to_owned();
        let m: Self = serde_json::from_value(value)
            .map_err(|e| WireError::Malformed(format!("{kind}: {e}")))?;
        m.check()
            .map_err(|why| WireError::Malformed(format!("{kind}: {why}")))?;
        Ok(m)
    }

    /// Writes one frame.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"))
    }

    /// The message's `type`.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Ask { .. } => "reach",
            Self::Result { .. } => "reach.result",
        }
    }

    /// The checks serde does not make: the id, and characters, never channels.
    fn check(&self) -> Result<(), &'static str> {
        let id_ok = |id: &str| !id.is_empty() && id.chars().count() <= MAX_ID_CHARS;
        match self {
            Self::Ask { id, from, to } => {
                if !id_ok(id) {
                    return Err("id");
                }
                if from.is_channel() || to.is_channel() {
                    return Err("a character, never a channel");
                }
                Ok(())
            }
            Self::Result { id, .. } => {
                if id_ok(id) {
                    Ok(())
                } else {
                    Err("id")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DirectoryMessage, Message};

    fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    fn malformed(text: &str) -> bool {
        matches!(ReachMessage::from_json(text), Err(WireError::Malformed(_)))
    }

    /// Spec: link messages: every reach message round-trips through its JSON form.
    #[test]
    fn every_reach_message_round_trips_through_its_json() {
        let all = [
            ReachMessage::Ask {
                id: "h7".into(),
                from: a("Bob@coldeve"),
                to: a("Ann@frost"),
            },
            ReachMessage::Ask {
                id: "h8".into(),
                from: a("Sir Bob@coldeve"),
                to: a("Ann@frost"),
            },
            ReachMessage::Result {
                id: "h7".into(),
                reachable: true,
            },
            ReachMessage::Result {
                id: "h8".into(),
                reachable: false,
            },
        ];
        for m in all {
            let json = m.to_json();
            assert!(
                json.contains(&format!("\"type\":\"{}\"", m.kind())),
                "{json}"
            );
            assert_eq!(ReachMessage::from_json(&json), Ok(m), "{json}");
        }
        assert_eq!(
            ReachMessage::Ask {
                id: "h1".into(),
                from: a("Bob@coldeve"),
                to: a("Ann@frost"),
            }
            .to_json(),
            r#"{"type":"reach","id":"h1","from":"Bob@coldeve","to":"Ann@frost"}"#
        );
    }

    /// Spec: link messages: a reader that knows only the other link messages or the directory's
    /// skips a reach message as a type it does not know, and the reach messages' reader skips
    /// theirs.
    #[test]
    fn a_reader_without_reach_skips_its_messages() {
        let frame = ReachMessage::Result {
            id: "h1".into(),
            reachable: true,
        }
        .to_json();
        let unknown = WireError::Unknown("reach.result".into());
        assert_eq!(Message::from_json(&frame), Err(unknown.clone()));
        assert_eq!(DirectoryMessage::from_json(&frame), Err(unknown));
        assert_eq!(
            ReachMessage::from_json(r#"{"type":"tell.result","id":"t1","outcome":"offline"}"#),
            Err(WireError::Unknown("tell.result".into()))
        );
    }

    /// Spec: link messages: a reach question names two characters, never a channel, under an id
    /// of 1 to 64 characters, and an answer says whether the character is reachable.
    #[test]
    fn a_reach_question_names_two_characters_and_its_answer_says_yes_or_no() {
        assert!(malformed(
            r#"{"type":"reach","id":"h1","from":"Bob@coldeve","to":"oldguard@channel"}"#
        ));
        assert!(malformed(
            r#"{"type":"reach","id":"h1","from":"oldguard@channel","to":"Ann@frost"}"#
        ));
        assert!(malformed(
            r#"{"type":"reach","id":"h1","from":"Bob","to":"Ann@frost"}"#
        ));
        assert!(malformed(
            r#"{"type":"reach","id":"","from":"Bob@coldeve","to":"Ann@frost"}"#
        ));
        assert!(malformed(&format!(
            r#"{{"type":"reach.result","id":"{}","reachable":true}}"#,
            "i".repeat(MAX_ID_CHARS + 1)
        )));
        assert!(malformed(r#"{"type":"reach.result","id":"h1"}"#));
        assert!(malformed(
            r#"{"type":"reach.result","id":"h1","reachable":"yes"}"#
        ));
        assert!(
            ReachMessage::from_json(
                r#"{"type":"reach","id":"h1","from":"Bob@coldeve","to":"Ann@frost","friend":true}"#
            )
            .is_ok(),
            "a field this version does not know is ignored"
        );
    }
}
